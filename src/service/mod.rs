/*!
For running a service over `Carrier`, a `Client` and a `Server` are required.
It is required to implement the given `Client` and `Server` traits for services that should
be running over `Carrier`.

`Carrier` will call `Server::spawn` whenever a remote `Peer` requests the service from the local
`Peer`. The remote `Peer` needs to run an instance of the `Client` service implementation.
*/
use error::*;
use peer::PeerBuilder;
use {NewStreamHandle, Stream};

use tokio_core::reactor::Handle;

use futures::Future;

use std::result;

pub(crate) mod executor;
pub mod lifeline;

pub type ServiceId = u64;

pub trait ServiceInstance: Future {
    /// A new incoming `Stream` for this `ServiceInstance`.
    fn incoming_stream(&mut self, stream: Stream);
}

/// Server side of a service.
pub trait Server {
    /// Start a new server instance of the service on the given connection.
    fn start(
        &mut self,
        handle: &Handle,
        stream: Stream,
        new_stream_handle: NewStreamHandle,
    ) -> Result<Box<ServiceInstance<Item = (), Error = Error>>>;
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

/// Client side of a service.
pub trait Client
where
    Self::Error: From<Error>,
{
    type Instance: ServiceInstance;
    type Error;
    /// Starts a new client instance.
    /// The returned `Future` should resolve, when the service is finished.
    fn start(
        self,
        handle: &Handle,
        stream: Stream,
        new_stream_handle: NewStreamHandle,
    ) -> result::Result<Self::Instance, Self::Error>;
    /// Returns the unique name of the service. The name will be used to identify this service.
    fn name(&self) -> &'static str;
}

pub fn register_builtin_services(builder: PeerBuilder) -> PeerBuilder {
    builder.register_service(lifeline::Lifeline {})
}
