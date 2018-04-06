use carrier::{self, Connection, Error, FileFormat, PubKeyHash, service::{Client, Server}};

use std::{result, thread, net::{IpAddr, SocketAddr}, sync::mpsc::channel};

use tokio_core::reactor::{Core, Handle};

use futures::{Future, Sink, Stream as FStream};

const TEST_SERVICE_DATA: &[u8] = b"HERP!DERP!TEST!SERVICE";

type Result<T> = result::Result<T, Error>;

pub fn start_bearer() {
    let (send, recv) = channel();

    thread::spawn(move || {
        let cert = include_bytes!("../../test_certs/bearer.cert.pem");
        let key = include_bytes!("../../test_certs/bearer.key.pem");

        let listen_port = 22222;
        let bearer_address: IpAddr = [127, 0, 0, 1].into();

        let peer_ca_vec = carrier::util::glob_for_certificates(format!(
            "{}/test_certs/trusted_peer_cas",
            env!("CARGO_MANIFEST_DIR")
        )).expect("Globbing for client certificate authorities(*.pem).");

        let mut evt_loop = Core::new().unwrap();

        let server =
            carrier::Bearer::builder(&evt_loop.handle(), (bearer_address, listen_port).into())
                .set_cert_chain(vec![cert.to_vec()], FileFormat::PEM)
                .set_private_key(key.to_vec(), FileFormat::PEM)
                .set_client_ca_cert_files(peer_ca_vec)
                .set_quic_listen_port(listen_port)
                .build()
                .unwrap();

        send.send(()).unwrap();
        server.run(&mut evt_loop).unwrap();
    });

    recv.recv().expect("Waiting for bearer to start");
}

pub fn start_peer() {
    let (send, recv) = channel();

    thread::spawn(move || {
        let bearer_addr: SocketAddr = ([127, 0, 0, 1], 22222).into();

        let cert = include_bytes!("../../test_certs/peer.cert.pem");
        let key = include_bytes!("../../test_certs/peer.key.pem");

        let peer_ca_vec = carrier::util::glob_for_certificates(format!(
            "{}/test_certs/trusted_peer_cas",
            env!("CARGO_MANIFEST_DIR")
        )).expect("Globbing for peer certificate authorities(*.pem).");

        let bearer_ca_vec = carrier::util::glob_for_certificates(format!(
            "{}/test_certs/trusted_cas",
            env!("CARGO_MANIFEST_DIR")
        )).expect("Globbing for bearer certificate authorities(*.pem).");

        let mut evt_loop = Core::new().unwrap();

        let builder = carrier::Peer::builder(&evt_loop.handle())
            .set_cert_chain(vec![cert.to_vec()], FileFormat::PEM)
            .set_private_key(key.to_vec(), FileFormat::PEM)
            .set_client_ca_cert_files(peer_ca_vec)
            .set_server_ca_cert_files(bearer_ca_vec)
            .register_service(TestService {});

        let builder = carrier::service::register_builtin_services(builder);

        let peer = evt_loop.run(builder.build(&bearer_addr).unwrap()).unwrap();

        send.send(()).unwrap();
        peer.run(&mut evt_loop).unwrap();
    });

    recv.recv().expect("Waiting for peer to start");
}

pub fn run_client() {
    let mut evt_loop = Core::new().unwrap();

    let bearer_addr: SocketAddr = ([127, 0, 0, 1], 22222).into();

    let cert = include_bytes!("../../test_certs/lifeline.cert.pem");
    let key = include_bytes!("../../test_certs/lifeline.key.pem");

    let peer_cert = include_bytes!("../../test_certs/peer.cert.pem");
    let peer_key =
        PubKeyHash::from_x509_pem(peer_cert, false).expect("Create peer key from peer cert.");
    println!("PEER: {}", peer_key);

    let builder = carrier::Peer::builder(&evt_loop.handle())
        .set_cert_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .build(&bearer_addr)
        .unwrap();

    let peer = evt_loop.run(builder).unwrap();

    let data = peer.run_service(&mut evt_loop, TestService {}, peer_key)
        .expect("TestService returns data.");

    assert_eq!(TEST_SERVICE_DATA.to_vec(), data);
}

struct TestService {}

impl Server for TestService {
    fn spawn(&mut self, handle: &Handle, mut con: Connection) -> Result<()> {
        con.start_send(TEST_SERVICE_DATA.into())?;
        con.poll_complete()?;

        //TODO: Ensure that picoquic delivers message before closing connections
        handle.spawn(con.for_each(|_| Ok(())).map_err(|_| ()));
        Ok(())
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}

impl Client for TestService {
    type Item = Vec<u8>;
    type Error = Error;
    type Future = Box<Future<Item = Self::Item, Error = Self::Error>>;

    fn start(self, _: &Handle, con: Connection) -> Result<Self::Future> {
        Ok(Box::new(
            con.into_future()
                .map_err(|e| e.0.into())
                .map(|(v, _)| v.unwrap().to_vec()),
        ))
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}
