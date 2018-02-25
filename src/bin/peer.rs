extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

use std::env::args;
use std::net::SocketAddr;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut evt_loop = Core::new().unwrap();
    let server_addr: SocketAddr = ([127, 0, 0, 1], 22222).into();

    let name = args().nth(1).expect("Please give name for this peer");

    let mut builder = carrier::Peer::build(
        &evt_loop.handle(),
        format!("{}/src/bin/cert.pem", manifest_dir),
        format!("{}/src/bin/key.pem", manifest_dir),
        name,
    ).unwrap();
    carrier::service::register_builtin_services(&mut builder);
    let peer = evt_loop.run(builder.register(&server_addr)).unwrap();
    peer.run(&mut evt_loop).unwrap();
}
