use carrier::{
    self,
    service::{Client, Server, Streams},
    Error, FileFormat, NewStreamHandle, PubKeyHash, SendFuture,
};

use std::{net::SocketAddr, result, sync::mpsc::channel, thread, time::Duration};

use tokio::runtime::Runtime;

use futures::{
    future, stream::futures_unordered, sync::mpsc::unbounded, Future, Sink, Stream as FStream,
};

const TEST_SERVICE_DATA: &[u8] = b"HERP!DERP!TEST!SERVICE";

type Result<T> = result::Result<T, Error>;

/// Starts the Bearer.
/// Returns the port the Bearer is listening on.
pub fn start_bearer() -> u16 {
    let (send, recv) = channel();

    thread::spawn(move || {
        let cert = include_bytes!("../../test_certs/bearer.cert.pem");
        let key = include_bytes!("../../test_certs/bearer.key.pem");

        let peer_ca_vec = carrier::util::glob_for_certificates(&format!(
            "{}/test_certs/trusted_peer_cas",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("Globbing for client certificate authorities(*.pem).");

        let evt_loop = Runtime::new().unwrap();

        let server = carrier::Peer::builder(evt_loop.executor())
            .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
            .set_private_key(key.to_vec(), FileFormat::PEM)
            .set_client_ca_cert_files(peer_ca_vec)
            .build()
            .unwrap();

        send.send(server.quic_local_addr()).unwrap();
        evt_loop.block_on_all(server).unwrap();
    });

    recv.recv().expect("Waiting for bearer to start").port()
}

/// Start the peer.
/// stream_num - The number of `Stream`s to start, 1 is minimum.
/// bearer_port - The port of the bearer.
pub fn start_peer(stream_num: u16, bearer_port: u16) {
    let (send, recv) = channel();

    thread::spawn(move || {
        let bearer_addr: SocketAddr = ([127, 0, 0, 1], bearer_port).into();

        let cert = include_bytes!("../../test_certs/peer.cert.pem");
        let key = include_bytes!("../../test_certs/peer.key.pem");

        let peer_ca_vec = carrier::util::glob_for_certificates(&format!(
            "{}/test_certs/trusted_peer_cas",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("Globbing for peer certificate authorities(*.pem).");

        let bearer_ca_vec = carrier::util::glob_for_certificates(&format!(
            "{}/test_certs/trusted_cas",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("Globbing for bearer certificate authorities(*.pem).");

        let evt_loop = Runtime::new().unwrap();

        let builder = carrier::Peer::builder(evt_loop.executor())
            .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
            .set_private_key(key.to_vec(), FileFormat::PEM)
            .set_client_ca_cert_files(peer_ca_vec)
            .set_server_ca_cert_files(bearer_ca_vec)
            .register_service(TestService::new(stream_num, 0))
            .add_remote_peer(bearer_addr);

        let builder = carrier::builtin_services::register(builder);

        let peer = builder.build().unwrap();

        send.send(()).unwrap();
        evt_loop.block_on_all(peer).unwrap();
    });

    recv.recv().expect("Waiting for peer to start");
}

/// Run the client.
/// stream_num - The number of `Stream`s the client should start
/// remote_stream_num - The number of remote `Stream`s the peer starts
/// bearer_port - The port of the bearer.
pub fn run_client(stream_num: u16, remote_stream_num: u16, bearer_port: u16) {
    let total_stream_num = (stream_num + remote_stream_num - 1) as usize;
    let mut evt_loop = Runtime::new().unwrap();

    let bearer_addr: SocketAddr = ([127, 0, 0, 1], bearer_port).into();

    let cert = include_bytes!("../../test_certs/lifeline.cert.pem");
    let key = include_bytes!("../../test_certs/lifeline.key.pem");

    let peer_cert = include_bytes!("../../test_certs/peer.cert.pem");
    let peer_key =
        PubKeyHash::from_x509_pem(peer_cert, false).expect("Create peer key from peer cert.");
    println!("PEER: {}", peer_key);

    let builder = carrier::Peer::builder(evt_loop.executor())
        .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .add_remote_peer(bearer_addr);

    let mut peer = builder.build().unwrap();

    for _ in 0..3 {
        let res = evt_loop.block_on(peer.run_service(
            TestService::new(stream_num, total_stream_num),
            peer_key.clone(),
        ));

        let data = match res {
            Ok(data) => data,
            Err(e) => match e {
                Error::PeerNotFound(_) => {
                    // Sleep and retry to connect to the peer afterwards
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
                e @ _ => panic!(e),
            },
        };

        assert_eq!(
            TEST_SERVICE_DATA
                .iter()
                .cloned()
                .cycle()
                .take(TEST_SERVICE_DATA.len() * total_stream_num)
                .collect::<Vec<_>>(),
            data
        );

        return;
    }

    panic!("Could not find requested peer!");
}

struct TestService {
    stream_num: u16,
    total_stream_num: usize,
}

impl TestService {
    fn new(stream_num: u16, total_stream_num: usize) -> TestService {
        TestService {
            stream_num,
            total_stream_num,
        }
    }
}

impl Server for TestService {
    fn start(&mut self, streams: Streams, mut new_stream_handle: NewStreamHandle) {
        let new_streams = (1..self.stream_num).map(|_| new_stream_handle.new_stream());

        tokio::spawn(
            streams
                .select(futures_unordered(new_streams))
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
    type Future = Box<SendFuture<Item = Vec<u8>, Error = Self::Error>>;

    fn start(
        self,
        streams: Streams,
        mut new_stream_handle: NewStreamHandle,
    ) -> Result<Self::Future> {
        let (send, recv) = unbounded();

        let new_streams = (1..self.stream_num).map(|_| new_stream_handle.new_stream());

        tokio::spawn(
            streams
                .select(futures_unordered(new_streams))
                .take(self.total_stream_num as u64)
                .for_each(move |stream| {
                    let send = send.clone();
                    tokio::spawn(stream.into_future().map_err(|_| ()).and_then(
                        move |(data, _)| {
                            let _ = send.unbounded_send(data.unwrap());
                            Ok(())
                        },
                    ));
                    Ok(())
                })
                .map_err(|_| ()),
        );

        Ok(Box::new(
            recv.fold(Vec::new(), |mut res, data| {
                res.extend(data);
                future::ok::<_, ()>(res)
            })
            .map_err(|_| Error::from("unknown")),
        ))
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}
