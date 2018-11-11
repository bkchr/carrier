extern crate carrier;
extern crate hole_punch;
extern crate tokio;

use carrier::builtin_services;

use tokio::runtime::Runtime;

use std::env::args;

fn main() {
    let mut evt_loop = Runtime::new().unwrap();

    let peer_key = args().nth(1).expect(
        "Please give the public key(sha256 hash as hex) of the peer you want to connect to.",
    );

    let peer_key = hole_punch::PubKeyHash::from_hashed_hex(&peer_key)
        .expect("Creates public key from hashed hex.");

    let bearer_addr = args()
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

    let client_ca_vec = carrier::util::glob_for_certificates(&client_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let server_ca_vec = carrier::util::glob_for_certificates(&server_ca_path)
        .expect("Globbing for server certificate authorities(*.pem).");

    let mut peer = carrier::Peer::builder(evt_loop.executor())
        .set_certificate_chain_file(cert)
        .set_private_key_file(key)
        .set_client_ca_cert_files(client_ca_vec)
        .set_server_ca_cert_files(server_ca_vec)
        .add_remote_peer(bearer_addr.clone())
        .build()
        .unwrap();

    evt_loop
        .block_on_all(peer.run_service(builtin_services::Lifeline::new(), peer_key))
        .unwrap();
}
