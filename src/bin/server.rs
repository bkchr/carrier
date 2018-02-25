extern crate carrier;
extern crate tokio_core;

use tokio_core::reactor::Core;

fn main() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut evt_loop = Core::new().unwrap();

    let server = carrier::Server::new(
        &evt_loop.handle(),
        format!("{}/src/bin/cert.pem", manifest_dir),
        format!("{}/src/bin/key.pem", manifest_dir),
        ([0, 0, 0, 0], 22222).into(),
    ).unwrap();

    server.run(&mut evt_loop).unwrap();
}
