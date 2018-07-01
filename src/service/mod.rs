/*!
For running a service over `Carrier`, a `Client` and a `Server` are required.
It is required to implement the given `Client` and `Server` traits for services that should
be running over `Carrier`.

`Carrier` will call `Server::spawn` whenever a remote `Peer` requests the service from the local
`Peer`. The remote `Peer` needs to run an instance of the `Client` service implementation.
*/
use peer::PeerBuilder;
use NewStreamHandle;

use tokio_core::reactor::Handle;

use futures::Future;

use std::result;

pub mod lifeline;
mod streams;

pub use self::streams::Streams;

pub type ServiceId = u64;

/// Server side of a service.
pub trait Server {
    /// Start a new server instance of the service.
    fn start(&mut self, handle: &Handle, streams: Streams, new_stream_handle: NewStreamHandle);
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

/// Client side of a service.
pub trait Client {
    type Error;
    type Future: Future<Error=Self::Error>;
    /// Starts a new client instance.
    /// The returned `Future` should resolve, when the service is finished.
    fn start(
        self,
        handle: &Handle,
        streams: Streams,
        new_stream_handle: NewStreamHandle,
    ) -> result::Result<Self::Future, Self::Error>;
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

pub fn register_builtin_services(builder: PeerBuilder) -> PeerBuilder {
    builder.register_service(lifeline::Lifeline {})
}
