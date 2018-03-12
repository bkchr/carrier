extern crate carrier;
extern crate hole_punch;
extern crate tokio_core;

use carrier::service;

use tokio_core::reactor::Core;

use std::env::args;
use std::net::SocketAddr;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut evt_loop = Core::new().unwrap();

    let peer_key = args().nth(1).expect(
        "Please give the public key(sha256 hash as hex) of the peer you want to connect to.",
    );

    let peer_key = hole_punch::PubKey::from_hashed_hex(&peer_key)
        .expect("Creates public key from hashed hex.");

    let server_addr: SocketAddr = args()
        .nth(2)
        .expect("Please give carrier server address as second argument.")
        .parse()
        .expect("Invalid server address");

    let trusted_server_certs_path = args()
        .nth(3)
        .expect("Please give path to trusted server certificates.");

    let trusted_client_certs_path = args()
        .nth(4)
        .expect("Please give path to trusted client certificates.");

    let trusted_server_certificates = carrier::util::glob_for_certificates(
        trusted_server_certs_path,
    ).expect("Globbing for trusted server certificates(*.pem).");

    let trusted_client_certificates = carrier::util::glob_for_certificates(
        trusted_client_certs_path,
    ).expect("Globbing for trusted client certificates(*.pem).");

    let builder = carrier::Peer::build(
        &evt_loop.handle(),
        format!("{}/src/bin/cert.pem", manifest_dir),
        format!("{}/src/bin/key.pem", manifest_dir),
        trusted_server_certificates,
        trusted_client_certificates,
    ).unwrap()
        .connect(&server_addr);

    let peer = evt_loop.run(builder).unwrap();

    peer.run_service(&mut evt_loop, service::lifeline::Lifeline::new(), peer_key)
        .unwrap()
}
