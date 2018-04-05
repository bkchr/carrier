extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::{env::var, net::IpAddr};

fn main() {
    let certificate_path =
        var("CARRIER_CERT_PATH").expect("Please give path to cert file via `CARRIER_CERT_PATH`");
    let key_path = var("CARRIER_KEY_PATH")
        .expect("Please give path to private key file via `CARRIER_KEY_PATH`");
    let listen_port = var("CARRIER_LISTEN_PORT")
        .map(|v| v.parse())
        .unwrap_or(Ok(22222))
        .expect("Integer value for `CARRIER_LISTEN_PORT`");
    let bearer_address: IpAddr = var("CARRIER_BEARER_ADDR")
        .expect(
            "Please specify bearer address \
             (public reachable address of this bearer) via `CARRIER_BEARER_ADDR`",
        )
        .parse()
        .expect("Invalid bearer address");
    let client_ca_path = var("CARRIER_CLIENT_CA_PATH").expect(
        "Please give path to client certificate authorities(*.pem) via `CARRIER_CLIENT_CA_PATH`",
    );

    let client_ca_vec = carrier::util::glob_for_certificates(client_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let mut evt_loop = Core::new().unwrap();

    let server = carrier::Bearer::builder(&evt_loop.handle(), (bearer_address, listen_port).into())
        .set_cert_chain_file(certificate_path)
        .set_private_key_file(key_path)
        .set_client_ca_cert_files(client_ca_vec)
        .build()
        .unwrap();

    println!("Bearer running (Port: {})", listen_port);
    server.run(&mut evt_loop).unwrap();
}
