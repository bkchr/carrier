use context::PeerContext;
use error::*;
use peer_builder::PeerBuilder;
use protocol::Protocol;
use service::Client;
use stream::{protocol_stream_create, ProtocolStream};

use std::net::SocketAddr;

use hole_punch::{Context, CreateConnectionToPeerHandle, PubKeyHash};

use futures::{
    sync::oneshot, Async::{NotReady, Ready}, Future, Poll, Sink, Stream as FStream,
};

use tokio_core::reactor::{Core, Handle};

/// Spawn the hole punch `Context`.
/// All incoming `Stream`s will be wrapped by the "incoming_stream_future" that processes the
/// `Stream`.
fn spawn_hole_punch_context(
    context: Context,
    peer_context: PeerContext,
    handle: &Handle,
) -> oneshot::Receiver<Result<()>> {
    let (sender, recv) = oneshot::channel();

    let inner_handle = handle.clone();
    handle.spawn(
        context
            .for_each(move |stream| {
                inner_handle.spawn(
                    build_incoming_stream_future(
                        protocol_stream_create(stream),
                        peer_context.clone(),
                        inner_handle.clone(),
                    ).map_err(|e| println!("IncomingStream error: {:?}", e)),
                );
                Ok(())
            })
            .then(|r| {
                let _ = sender.send(r.map_err(|e| e.into()));
                Ok(())
            }),
    );

    recv
}

/// The `Peer` is the running instance.
/// It handles all registered services and is also responsible for spawning new service instances.
pub struct Peer {
    handle: Handle,
    peer_context: PeerContext,
    context_result: oneshot::Receiver<Result<()>>,
    create_connection_to_peer_handle: CreateConnectionToPeerHandle,
    quic_local_addr: SocketAddr,
}

impl Peer {
    pub(crate) fn new(handle: Handle, context: Context, peer_context: PeerContext) -> Peer {
        let create_connection_to_peer_handle = context.create_connection_to_peer_handle();

        let quic_local_addr = context.quic_local_addr();
        let context_result = spawn_hole_punch_context(context, peer_context.clone(), &handle);

        Peer {
            handle,
            peer_context,
            context_result,
            create_connection_to_peer_handle,
            quic_local_addr,
        }
    }

    /// Create a `PeerBuilder` for building a `Peer` instance.
    pub fn builder(handle: Handle) -> PeerBuilder {
        PeerBuilder::new(handle)
    }

    /// Run this `Peer`.
    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }

    /// Connect to the given `Peer` and run the given `Service` (locally and remotely).
    pub fn run_service<S: Client>(
        &mut self,
        service: S,
        peer: PubKeyHash,
    ) -> impl Future<Item = <S::Future as Future>::Item, Error = S::Error>
    where
        S::Error: From<Error>,
    {
        let name = service.name();
        let local_service_id = self.peer_context.next_service_id();
        let handle = self.handle.clone();
        let mut peer_context = self.peer_context.clone();

        self.create_connection_to_peer_handle
            .create_connection_to_peer(peer)
            .map_err(|e| Error::from(e))
            .and_then(move |stream| {
                let stream = protocol_stream_create(stream);
                stream
                    .send(Protocol::RequestServiceStart {
                        name: name.into(),
                        local_id: local_service_id,
                    })
                    .and_then(|s| s.into_future().map_err(|e| e.0))
                    .map_err(|e| Error::from(e))
            })
            .and_then(move |(msg, stream)| match msg {
                None => bail!("Stream closed while requesting service!"),
                Some(Protocol::ServiceStarted { id }) => Ok((id, stream)),
                Some(Protocol::ServiceNotFound) => bail!("Requested service({}) not found!", name),
                _ => bail!("Received not expected message!"),
            })
            .map_err(|e| e.into())
            .and_then(move |(id, stream)| {
                peer_context.start_client_service_instance(
                    service,
                    local_service_id,
                    id,
                    stream.into(),
                    &handle,
                )
            })
            .flatten()
    }

    /// The local address of the Quic backend.
    pub fn quic_local_addr(&self) -> SocketAddr {
        self.quic_local_addr
    }
}

impl Future for Peer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.context_result.poll() {
            Err(_) => Ok(Ready(())),
            Ok(Ready(res)) => Ok(Ready(res?)),
            Ok(NotReady) => Ok(NotReady),
        }
    }
}

fn build_incoming_stream_future(
    stream: ProtocolStream,
    mut context: PeerContext,
    handle: Handle,
) -> impl Future<Item = (), Error = Error> {
    stream
        .into_future()
        .map_err(|e| e.0.into())
        .and_then(move |(msg, stream)| match msg {
            None => Ok(()),
            Some(Protocol::ConnectToService { id }) => {
                context.connect_stream_to_service_instance(stream.into(), id);
                Ok(())
            }
            Some(Protocol::RequestServiceStart { name, local_id }) => {
                context.start_server_service_instance(&name, local_id, stream.into(), &handle);
                Ok(())
            }
            _ => bail!("Unexpected message at incoming Stream."),
        })
}
