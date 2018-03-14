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

    let trusted_client_certificates_path = var("CARRIER_TRUSTED_CLIENT_CERTS_PATH")
        .expect("Please give path to the trusted client certificates(*.pem) via `CARRIER_TRUSTED_CLIENT_CERTS_PATH`");

    let trusted_server_certificates_path = var("CARRIER_TRUSTED_SERVER_CERTS_PATH")
        .expect("Please give path to the trusted server certificates(*.pem) via `CARRIER_TRUSTED_SERVER_CERTS_PATH`");

    let trusted_client_certificates = carrier::util::glob_for_certificates(
        trusted_client_certificates_path,
    ).expect("Globbing for trusted client certificates(*.pem).");

    let trusted_server_certificates = carrier::util::glob_for_certificates(
        trusted_server_certificates_path,
    ).expect("Globbing for trusted server certificates(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let mut builder = carrier::Peer::build(
        &evt_loop.handle(),
        certificate_path,
        key_path,
        trusted_server_certificates,
        trusted_client_certificates,
    ).unwrap();

    carrier::service::register_builtin_services(&mut builder);

    println!("Peer connects to bearer({})", server_addr);
    let peer = evt_loop
        .run(builder.connect(&server_addr).unwrap())
        .unwrap();

    println!("Peer running");
    peer.run(&mut evt_loop).unwrap();
}
