extern crate carrier;
extern crate pretty_env_logger;
extern crate tokio;
#[macro_use]
extern crate log;

use tokio::runtime::Runtime;

use std::env::var;

fn main() {
    pretty_env_logger::init();

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

    let client_ca_vec = carrier::util::glob_for_certificates(&peer_ca_path)
        .expect("Globbing for client certificate authorities(*.pem).");

    let server_ca_vec = carrier::util::glob_for_certificates(&bearer_ca_path)
        .expect("Globbing for server certificate authorities(*.pem).");

    let evt_loop = Runtime::new().unwrap();

    let builder = carrier::Peer::builder(evt_loop.executor())
        .set_certificate_chain_file(certificate_path)
        .set_private_key_file(key_path)
        .set_client_ca_cert_files(client_ca_vec)
        .set_server_ca_cert_files(server_ca_vec)
        .add_remote_peer(bearer_addr.clone());

    let builder = carrier::builtin_services::register(builder);

    info!("Peer connects to bearer({})", bearer_addr);
    let peer = builder.build().unwrap();

    info!("Peer running");
    evt_loop.block_on_all(peer).unwrap();
}
