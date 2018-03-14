extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::env::var;

fn main() {
    let server_addr = var("CARRIER_SERVER_ADDR")
        .expect("Please give carrier server address via `CARRIER_SERVER_ADDR`");
    let certificate_path =
        var("CARRIER_CERT_PATH").expect("Please give path to cert file via `CARRIER_CERT_PATH`");
    let key_path = var("CARRIER_KEY_PATH")
        .expect("Please give path to private key file via `CARRIER_KEY_PATH`");

    let client_ca_path = var("CARRIER_CLIENT_CA_PATH").expect(
        "Please give path to client certificate authorities(*.pem) via `CARRIER_CLIENT_CA_PATH`",
    );

    let server_ca_path = var("CARRIER_SERVER_CA_PATH").expect(
        "Please give path to server certificate authorities(*.pem) via `CARRIER_SERVER_CA_PATH`",
    );

    let client_ca_vec = carrier::util::glob_for_certificates(client_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let server_ca_vec = carrier::util::glob_for_certificates(server_ca_path)
        .expect("Globbing for server certificate authorities(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let mut builder = carrier::Peer::build(
        &evt_loop.handle(),
        certificate_path,
        key_path,
        server_ca_vec,
        client_ca_vec,
    ).unwrap();

    carrier::service::register_builtin_services(&mut builder);

    println!("Peer connects to bearer({})", server_addr);
    let peer = evt_loop
        .run(builder.connect(&server_addr).unwrap())
        .unwrap();

    println!("Peer running");
    peer.run(&mut evt_loop).unwrap();
}
