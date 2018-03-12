extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::env::var;

fn main() {
    let certificate_path =
        var("CARRIER_CERT_PATH").expect("Please give path to cert file via `CARRIER_CERT_PATH`");
    let key_path = var("CARRIER_KEY_PATH")
        .expect("Please give path to private key file via `CARRIER_KEY_PATH`");
    let listen_port = var("CARRIER_LISTEN_PORT")
        .map(|v| v.parse())
        .unwrap_or(Ok(22222))
        .expect("Integer value for `CARRIER_LISTEN_PORT`");
    let trusted_client_certificates_path = var("CARRIER_TRUSTED_CLIENT_CERTS_PATH")
        .expect("Please give path to the trusted client certificates(*.pem) via `CARRIER_TRUSTED_CLIENT_CERTS_PATH`");

    let trusted_client_certificates = carrier::util::glob_for_certificates(
        trusted_client_certificates_path,
    ).expect("Globbing for trusted client certificates(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let server = carrier::Server::new(
        &evt_loop.handle(),
        certificate_path,
        key_path,
        ([0, 0, 0, 0], listen_port).into(),
        trusted_client_certificates,
    ).unwrap();

    server.run(&mut evt_loop).unwrap();
}
