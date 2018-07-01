use carrier::{
    self, service::{Client, Server, Streams}, Error, FileFormat, NewStreamHandle, PubKeyHash,
};

use std::{net::SocketAddr, result, sync::mpsc::channel, thread};

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

        let peer_ca_vec = carrier::util::glob_for_certificates(format!(
            "{}/test_certs/trusted_peer_cas",
            env!("CARGO_MANIFEST_DIR")
        )).expect("Globbing for client certificate authorities(*.pem).");

        let mut evt_loop = Core::new().unwrap();

        let server = carrier::Peer::builder(evt_loop.handle())
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

        let builder = carrier::Peer::builder(evt_loop.handle())
            .set_cert_chain(vec![cert.to_vec()], FileFormat::PEM)
            .set_private_key(key.to_vec(), FileFormat::PEM)
            .set_client_ca_cert_files(peer_ca_vec)
            .set_server_ca_cert_files(bearer_ca_vec)
            .register_service(TestService {})
            .add_remote_peer(bearer_addr)
            .unwrap();

        let builder = carrier::service::register_builtin_services(builder);

        let peer = builder.build().unwrap();

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

    let builder = carrier::Peer::builder(evt_loop.handle())
        .set_cert_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .add_remote_peer(bearer_addr)
        .unwrap();

    let mut peer = builder.build().unwrap();

    let data = evt_loop
        .run(peer.run_service(TestService {}, peer_key))
        .expect("TestService returns data.");

    assert_eq!(TEST_SERVICE_DATA.to_vec(), data);
}

struct TestService {}

impl Server for TestService {
    fn start(&mut self, handle: &Handle, streams: Streams, new_stream_handle: NewStreamHandle) {
        handle.spawn(
            streams
                .for_each(|mut stream| {
                    stream.start_send(TEST_SERVICE_DATA.into()).unwrap();
                    stream.poll_complete().unwrap();
                    Ok(())
                })
                .map_err(|e| panic!(e)),
        );
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}

impl Client for TestService {
    type Error = Error;
    type Future = Box<Future<Item = Vec<u8>, Error = Self::Error>>;

    fn start(
        self,
        _: &Handle,
        streams: Streams,
        new_stream_handle: NewStreamHandle,
    ) -> Result<Self::Future> {
        Ok(Box::new(
            streams
                .into_future()
                .map_err(|e| e.0.into())
                .and_then(|(stream, _)| {
                    stream
                        .unwrap()
                        .into_future()
                        .map_err(|e| e.0.into())
                        .map(|(v, _)| v.unwrap().to_vec())
                }),
        ))
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}
