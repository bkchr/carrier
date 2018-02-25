use error::*;
use peer::{BuildPeer, PeerBuilder};

use hole_punch::plain::Stream;

use tokio_core::reactor::{Core, Handle};

pub mod lifeline;

pub trait Service {
    fn spawn(&mut self, handle: &Handle, con: Stream) -> Result<()>;
    fn run(self, evt_loop: &mut Core, peer: BuildPeer, name: &str) -> Result<()>;
    fn name(&self) -> String;
}

pub fn register_builtin_services(builder: &mut PeerBuilder) {
    builder.register_service(lifeline::Lifeline {});
}
