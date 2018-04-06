extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::env::var;

fn main() {
    let bearer_addr = var("CARRIER_SERVER_ADDR")
        .expect("Please give carrier server address via `CARRIER_SERVER_ADDR`");
    let certificate_path =
        var("CARRIER_CERT_PATH").expect("Please give path to cert file via `CARRIER_CERT_PATH`");
    let key_path = var("CARRIER_KEY_PATH")
        .expect("Please give path to private key file via `CARRIER_KEY_PATH`");

    let peer_ca_path = var("CARRIER_CLIENT_CA_PATH").expect(
        "Please give path to client certificate authorities(*.pem) via `CARRIER_CLIENT_CA_PATH`",
    );

    let bearer_ca_path = var("CARRIER_SERVER_CA_PATH").expect(
        "Please give path to server certificate authorities(*.pem) via `CARRIER_SERVER_CA_PATH`",
    );

    let client_ca_vec = carrier::util::glob_for_certificates(peer_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let server_ca_vec = carrier::util::glob_for_certificates(bearer_ca_path)
        .expect("Globbing for server certificate authorities(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let builder = carrier::Peer::builder(&evt_loop.handle())
        .set_cert_chain_file(certificate_path)
        .set_private_key_file(key_path)
        .set_client_ca_cert_files(client_ca_vec)
        .set_server_ca_cert_files(server_ca_vec);

    let builder = carrier::service::register_builtin_services(builder);

    println!("Peer connects to bearer({})", bearer_addr);
    let peer = evt_loop.run(builder.build(&bearer_addr).unwrap()).unwrap();

    println!("Peer running");
    peer.run(&mut evt_loop).unwrap();
}
