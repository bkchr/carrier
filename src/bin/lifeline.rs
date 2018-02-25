extern crate carrier;
extern crate tokio_core;

use carrier::service::{self, Service};

use tokio_core::reactor::Core;

use std::env::args;
use std::net::SocketAddr;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut evt_loop = Core::new().unwrap();
    let server_addr: SocketAddr = ([127, 0, 0, 1], 22222).into();

    let name = args()
        .nth(1)
        .expect("Please give the name of the other peer.");

    let builder = carrier::Peer::build(
        &evt_loop.handle(),
        format!("{}/src/bin/cert.pem", manifest_dir),
        format!("{}/src/bin/key.pem", manifest_dir),
        "dev".into(),
    ).unwrap();

    service::lifeline::Lifeline::new()
        .run(&mut evt_loop, builder.login(&server_addr, "test".into()), &name)
        .unwrap();
}
