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

    let peer_key = hole_punch::PubKeyHash::from_hashed_hex(&peer_key)
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

    let builder = carrier::Peer::builder(&evt_loop.handle())
        .set_cert_chain_file(cert)
        .set_private_key_file(key)
        .set_client_ca_cert_files(client_ca_vec)
        .set_server_ca_cert_files(server_ca_vec)
        .build(&server_addr)
        .unwrap();

    let peer = evt_loop.run(builder).unwrap();

    peer.run_service(&mut evt_loop, service::lifeline::Lifeline::new(), peer_key)
        .unwrap()
}
