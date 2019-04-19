use context::PeerContext;
use error::*;
use peer_builder::PeerBuilder;
use protocol::Protocol;
use service::Client;

use std::net::SocketAddr;

use hole_punch::{Context, CreateConnectionToPeerHandle, ProtocolStream, PubKeyHash, SendFuture};

use futures::{
    sync::oneshot,
    try_ready,
    Async::{NotReady, Ready},
    Future, Poll, Sink, Stream as FStream,
};

use tokio::runtime::TaskExecutor;

struct HolePunchContextRunner {
    context: Context,
    handle: Option<oneshot::Sender<()>>,
    peer_context: PeerContext,
}

impl HolePunchContextRunner {
    fn new(context: Context, handle: oneshot::Sender<()>, peer_context: PeerContext) -> Self {
        Self {
            context,
            handle: Some(handle),
            peer_context,
        }
    }
}

impl Future for HolePunchContextRunner {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self
            .handle
            .as_mut()
            .and_then(|h| h.poll_cancel().map(|r| r.is_ready()).ok())
            .unwrap_or(true)
        {
            return Ok(Ready(()));
        }

        loop {
            let stream = match try_ready!(self.context.poll().map_err(|e| error!("{:?}", e))) {
                Some(stream) => stream,
                None => {
                    error!("Holepunch context returned `None`!");
                    return Ok(Ready(()));
                }
            };

            tokio::spawn(
                build_incoming_stream_future(stream.into(), self.peer_context.clone())
                    .map_err(|e| error!("IncomingStream error: {:?}", e)),
            );
        }
    }
}

/// Spawn the hole punch `Context`.
/// All incoming `Stream`s will be wrapped by the "incoming_stream_future" that processes the
/// `Stream`.
fn spawn_hole_punch_context(
    context: Context,
    peer_context: PeerContext,
    handle: TaskExecutor,
) -> oneshot::Receiver<()> {
    let (sender, recv) = oneshot::channel();
    handle.spawn(HolePunchContextRunner::new(context, sender, peer_context));
    recv
}

/// The `Peer` is the running instance.
/// It handles all registered services and is also responsible for spawning new service instances.
pub struct Peer {
    peer_context: PeerContext,
    context_result: oneshot::Receiver<()>,
    create_connection_to_peer_handle: CreateConnectionToPeerHandle,
    quic_local_addr: SocketAddr,
}

impl Peer {
    pub(crate) fn new(handle: TaskExecutor, context: Context, peer_context: PeerContext) -> Peer {
        let create_connection_to_peer_handle = context.create_connection_to_peer_handle();

        let quic_local_addr = context.quic_local_addr();
        let context_result = spawn_hole_punch_context(context, peer_context.clone(), handle);

        Peer {
            peer_context,
            context_result,
            create_connection_to_peer_handle,
            quic_local_addr,
        }
    }

    /// Create a `PeerBuilder` for building a `Peer` instance.
    pub fn builder(handle: TaskExecutor) -> PeerBuilder {
        PeerBuilder::new(handle)
    }

    /// Connect to the given `Peer` and run the given `Service` (locally and remotely).
    pub fn run_service<S: Client>(
        &mut self,
        service: S,
        peer: PubKeyHash,
    ) -> impl SendFuture<Item = <S::Future as Future>::Item, Error = S::Error>
    where
        S::Error: From<Error>,
    {
        let name = service.name();
        let local_service_id = self.peer_context.next_service_id();
        let mut peer_context = self.peer_context.clone();

        self.create_connection_to_peer_handle
            .create_connection_to_peer(peer)
            .map_err(Error::from)
            .and_then(move |stream| {
                let stream: ProtocolStream<Protocol> = stream.into();
                stream
                    .send(Protocol::RequestServiceStart {
                        name: name.into(),
                        local_id: local_service_id,
                    })
                    .and_then(|s| s.into_future().map_err(|e| e.0))
                    .map_err(Into::into)
            })
            .and_then(move |(msg, stream)| match msg {
                None => bail!("Stream closed while requesting service!"),
                Some(Protocol::ServiceStarted { id }) => Ok((id, stream)),
                Some(Protocol::ServiceNotFound) => bail!("Requested service({}) not found!", name),
                _ => bail!("Received not expected message!"),
            })
            .map_err(Into::into)
            .and_then(move |(id, stream)| {
                peer_context.start_client_service_instance(
                    service,
                    local_service_id,
                    id,
                    stream.into(),
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
            Ok(Ready(())) => Ok(Ready(())),
            Ok(NotReady) => Ok(NotReady),
        }
    }
}

fn build_incoming_stream_future(
    stream: ProtocolStream<Protocol>,
    mut context: PeerContext,
) -> impl SendFuture<Item = (), Error = Error> {
    stream
        .into_future()
        .map_err(|e| e.0.into())
        .and_then(move |(msg, stream)| match msg {
            None => Ok(()),
            Some(Protocol::ConnectToService { id }) => {
                context.connect_stream_to_service_instance(stream, id);
                Ok(())
            }
            Some(Protocol::RequestServiceStart { name, local_id }) => {
                context.start_server_service_instance(&name, local_id, stream);
                Ok(())
            }
            _ => bail!("Unexpected message at incoming Stream."),
        })
}
