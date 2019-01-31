extern crate carrier;
extern crate futures;
extern crate tokio;

use tokio::runtime::Runtime;

mod common;

#[test]
fn peer_connects_to_peer() {
    let mut runtime = Runtime::new().expect("Creates runtime");

    let port = common::start_bearer(runtime.executor());
    let peer_streams = 1;
    common::start_peer(peer_streams, port, runtime.executor());
    common::run_client(1, peer_streams, port, &mut runtime);
}

#[test]
fn peer_connects_to_peer_with_2_client_streams() {
    let mut runtime = Runtime::new().expect("Creates runtime");

    let port = common::start_bearer(runtime.executor());
    let peer_streams = 1;
    common::start_peer(peer_streams, port, runtime.executor());
    common::run_client(2, peer_streams, port, &mut runtime);
}

#[test]
fn peer_connects_to_peer_with_2_peer_streams() {
    let mut runtime = Runtime::new().expect("Creates runtime");

    let port = common::start_bearer(runtime.executor());
    let peer_streams = 2;
    common::start_peer(peer_streams, port, runtime.executor());
    common::run_client(1, peer_streams, port, &mut runtime);
}
