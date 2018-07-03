extern crate carrier;
extern crate futures;
extern crate tokio_core;

mod common;

#[test]
fn peer_connects_to_peer() {
    let port = common::start_bearer();
    let peer_streams = 1;
    common::start_peer(peer_streams, port);
    common::run_client(1, peer_streams, port);
}

#[test]
fn peer_connects_to_peer_with_2_client_streams() {
    let port = common::start_bearer();
    let peer_streams = 1;
    common::start_peer(peer_streams, port);
    common::run_client(2, peer_streams, port);
}

#[test]
fn peer_connects_to_peer_with_2_peer_streams() {
    let port = common::start_bearer();
    let peer_streams = 2;
    common::start_peer(peer_streams, port);
    common::run_client(1, peer_streams, port);
}
