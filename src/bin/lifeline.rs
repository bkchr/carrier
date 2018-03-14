extern crate carrier;
extern crate hole_punch;
extern crate tokio_core;

use carrier::service;

use tokio_core::reactor::Core;

use std::env::args;

fn main() {
    let mut evt_loop = Core::new().unwrap();

    let peer_key = args().nth(1).expect(
        "Please give the public key(sha256 hash as hex) of the peer you want to connect to.",
    );

    let peer_key = hole_punch::PubKey::from_hashed_hex(&peer_key)
        .expect("Creates public key from hashed hex.");

    let server_addr = args()
        .nth(2)
        .expect("Please give carrier server address as second argument.");

    let cert = args().nth(3).expect("Please give path to certificate.");

    let key = args().nth(4).expect("Please give path to key.");

    let client_ca_path = args()
        .nth(5)
        .expect("Please give path to client certificate authorities(*.pem)");

    let server_ca_path = args()
        .nth(6)
        .expect("Please give path to server certificate authorities(*.pem)");

    let client_ca_vec = carrier::util::glob_for_certificates(client_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let server_ca_vec = carrier::util::glob_for_certificates(server_ca_path)
        .expect("Globbing for server certificate authorities(*.pem).");

    let builder = carrier::Peer::build(&evt_loop.handle(), cert, key, server_ca_vec, client_ca_vec)
        .unwrap()
        .connect(&server_addr)
        .unwrap();

    let peer = evt_loop.run(builder).unwrap();

    peer.run_service(&mut evt_loop, service::lifeline::Lifeline::new(), peer_key)
        .unwrap()
}
