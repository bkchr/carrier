extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::env::var;
use std::net::SocketAddr;

fn main() {
    let server_addr: SocketAddr = var("CARRIER_SERVER_ADDR")
        .expect("Please give carrier server address via `CARRIER_SERVER_ADDR`")
        .parse()
        .expect("Invalid server address");
    let certificate_path =
        var("CARRIER_CERT_PATH").expect("Please give path to cert file via `CARRIER_CERT_PATH`");
    let key_path = var("CARRIER_KEY_PATH")
        .expect("Please give path to private key file via `CARRIER_KEY_PATH`");
    let peer_name =
        var("CARRIER_PEER_NAME").expect("Please give carrier peer name via `CARRIER_PEER_NAME`");

    let mut evt_loop = Core::new().unwrap();

    let mut builder =
        carrier::Peer::build(&evt_loop.handle(), certificate_path, key_path, peer_name).unwrap();

    carrier::service::register_builtin_services(&mut builder);
    let peer = evt_loop.run(builder.register(&server_addr)).unwrap();
    peer.run(&mut evt_loop).unwrap();
}
