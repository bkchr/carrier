extern crate carrier;
extern crate futures;
extern crate tokio_core;

mod common;

#[test]
fn main() {
    common::start_bearer();
    common::start_peer();
    common::run_client();
}
