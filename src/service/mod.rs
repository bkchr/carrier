/*!
For running a service over `Carrier`, a `Client` and a `Server` are required.
It is required to implement the given `Client` and `Server` traits for services that should
be running over `Carrier`.

`Carrier` will call `Server::spawn` whenever a remote `Peer` requests the service from the local
`Peer`. The remote `Peer` needs to run an instance of the `Client` service implementation.
*/
use error::*;
use peer::PeerBuilder;
use super::Connection;

use tokio_core::reactor::Handle;

use futures::Future;

use std::result;

pub mod lifeline;

/// Server side of a service.
pub trait Server {
    /// Spawn a new server instance of the service on the given connection.
    fn spawn(&mut self, handle: &Handle, con: Connection) -> Result<()>;
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

/// Client side of a service.
pub trait Client {
    type Item;
    type Error;
    type Future: Future<Item = Self::Item, Error = Self::Error>;
    /// Starts a new client instance of the service on the given connection.
    /// The returned `Future` should resolve, when the service is finished.
    fn start(self, handle: &Handle, con: Connection) -> result::Result<Self::Future, Self::Error>;
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

pub fn register_builtin_services(builder: PeerBuilder) -> PeerBuilder {
    builder.register_service(lifeline::Lifeline {})
}
