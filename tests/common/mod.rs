use carrier::{
    self,
    service::{Client, Server, Streams},
    Error, FileFormat, NewStreamHandle, PubKeyHash, SendFuture,
};

use std::{net::SocketAddr, result, thread, time::Duration};

use tokio::runtime::{Runtime, TaskExecutor};

use futures::{
    future, future::FutureResult, stream::futures_unordered, sync::mpsc::unbounded, Future, Sink,
    Stream as FStream,
};

const TEST_SERVICE_DATA: &[u8] = b"HERP!DERP!TEST!SERVICE";

type Result<T> = result::Result<T, Error>;

/// Starts the Bearer.
/// Returns the port the Bearer is listening on.
pub fn start_bearer(executor: TaskExecutor) -> u16 {
    let cert = include_bytes!("../../test_certs/bearer.cert.pem");
    let key = include_bytes!("../../test_certs/bearer.key.pem");

    let peer_ca_vec = carrier::util::glob_for_certificates(&format!(
        "{}/test_certs/trusted_peer_cas",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("Globbing for client certificate authorities(*.pem).");

    let server = carrier::Peer::builder(executor.clone())
        .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .set_client_ca_cert_files(peer_ca_vec)
        .build()
        .unwrap();

    let local_addr = server.quic_local_addr();
    executor.spawn(server.map_err(|e| panic!(e)));

    local_addr.port()
}

/// Start the peer.
/// stream_num - The number of `Stream`s to start, 1 is minimum.
/// bearer_port - The port of the bearer.
pub fn start_peer(stream_num: u16, bearer_port: u16, send_data: bool, executor: TaskExecutor) {
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

    let builder = carrier::Peer::builder(executor.clone())
        .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .set_client_ca_cert_files(peer_ca_vec)
        .set_server_ca_cert_files(bearer_ca_vec)
        .register_service(TestService::new(stream_num, 0, send_data))
        .add_remote_peer(bearer_addr);

    let builder = carrier::builtin_services::register(builder);

    let peer = builder.build().unwrap();
    executor.spawn(peer.map_err(|e| panic!(e)));
}

/// Run the client.
/// stream_num - The number of `Stream`s the client should start
/// remote_stream_num - The number of remote `Stream`s the peer starts
/// bearer_port - The port of the bearer.
pub fn run_client_with_test(
    stream_num: u16,
    remote_stream_num: u16,
    bearer_port: u16,
    runtime: &mut Runtime,
) {
    let total_stream_num = (stream_num + remote_stream_num - 1) as usize;

    let bearer_addr: SocketAddr = ([127, 0, 0, 1], bearer_port).into();

    let cert = include_bytes!("../../test_certs/lifeline.cert.pem");
    let key = include_bytes!("../../test_certs/lifeline.key.pem");

    let peer_cert = include_bytes!("../../test_certs/peer.cert.pem");
    let peer_key =
        PubKeyHash::from_x509_pem(peer_cert, false).expect("Create peer key from peer cert.");
    println!("PEER: {}", peer_key);

    let builder = carrier::Peer::builder(runtime.executor())
        .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .add_remote_peer(bearer_addr);

    let mut peer = builder.build().unwrap();

    for _ in 0..3 {
        let res = runtime.block_on(
            peer.run_service(
                TestService::new(stream_num, total_stream_num, false),
                peer_key.clone(),
            )
            .flatten(),
        );

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

pub fn run_client(
    stream_num: u16,
    remote_stream_num: u16,
    bearer_port: u16,
    runtime: &mut Runtime,
) -> (carrier::Peer, impl Future<Item = Vec<u8>, Error = Error>) {
    let total_stream_num = (stream_num + remote_stream_num - 1) as usize;

    let bearer_addr: SocketAddr = ([127, 0, 0, 1], bearer_port).into();

    let cert = include_bytes!("../../test_certs/lifeline.cert.pem");
    let key = include_bytes!("../../test_certs/lifeline.key.pem");

    let peer_cert = include_bytes!("../../test_certs/peer.cert.pem");
    let peer_key =
        PubKeyHash::from_x509_pem(peer_cert, false).expect("Create peer key from peer cert.");
    println!("PEER: {}", peer_key);

    let builder = carrier::Peer::builder(runtime.executor())
        .set_certificate_chain(vec![cert.to_vec()], FileFormat::PEM)
        .set_private_key(key.to_vec(), FileFormat::PEM)
        .add_remote_peer(bearer_addr);

    let mut peer = builder.build().unwrap();

    for _ in 0..3 {
        let service = runtime.block_on(peer.run_service(
            TestService::new(stream_num, total_stream_num, false),
            peer_key.clone(),
        ));

        let service = match service {
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

        return (peer, service);
    }

    panic!("Could not find requested peer");
}

struct TestService {
    stream_num: u16,
    total_stream_num: usize,
    send_data: bool,
}

impl TestService {
    fn new(stream_num: u16, total_stream_num: usize, send_data: bool) -> TestService {
        TestService {
            stream_num,
            total_stream_num,
            send_data,
        }
    }
}

impl Server for TestService {
    fn start(&mut self, streams: Streams, mut new_stream_handle: NewStreamHandle) {
        let new_streams = (1..self.stream_num).map(|_| new_stream_handle.new_stream());
        let send_data = self.send_data;

        tokio::spawn(
            streams
                .select(futures_unordered(new_streams))
                .for_each(move |mut stream| {
                    if send_data {
                        stream.start_send(TEST_SERVICE_DATA.into()).unwrap();
                        stream.poll_complete().unwrap();
                    } else {
                        tokio::spawn(stream.into_future().map(|_| ()).map_err(|_| ()));
                    }
                    Ok(())
                })
                .map_err(|e| println!("{:?}", e)),
        );
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}

impl Client for TestService {
    type Error = Error;
    type Future = FutureResult<Box<SendFuture<Item = Vec<u8>, Error = Self::Error>>, Error>;

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
                    tokio::spawn(
                        stream
                            .for_each(move |data| {
                                let _ = send.unbounded_send(data);
                                Ok(())
                            })
                            .map_err(|_| ())
                            .and_then(|_| {
                                Ok(())
                            }),
                    );
                    Ok(())
                })
                .map_err(|_| ()),
        );

        Ok(future::ok(Box::new(
            recv.fold(Vec::new(), |mut res, data| {
                res.extend(data);
                future::ok::<_, ()>(res)
            })
            .map_err(|_| Error::from("unknown")),
        )))
    }

    fn name(&self) -> &'static str {
        "testservice"
    }
}
