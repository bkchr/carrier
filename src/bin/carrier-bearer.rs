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
    let client_ca_path = var("CARRIER_CLIENT_CA_PATH").expect(
        "Please give path to client certificate authorities(*.pem) via `CARRIER_CLIENT_CA_PATH`",
    );

    let client_ca_vec = carrier::util::glob_for_certificates(client_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let builder = carrier::Peer::builder(evt_loop.handle())
        .set_quic_listen_port(listen_port)
        .set_certificate_chain_file(certificate_path)
        .set_private_key_file(key_path)
        .set_client_ca_cert_files(client_ca_vec);

    let builder = carrier::builtin_services::register(builder);

    println!("Bearer running (Port: {})", listen_port);
    let bearer = builder.build().unwrap();

    bearer.run(&mut evt_loop).unwrap();
}
