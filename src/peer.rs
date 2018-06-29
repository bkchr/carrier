use context::PeerContext;
use error::*;
use protocol::Protocol;
use service::{Client, Server, ServiceId, ServiceInstance};
use stream::{protocol_stream_create, NewStreamHandle, ProtocolStream, Stream};

use std::{
    fs::File, io::Read, net::ToSocketAddrs, path::{Path, PathBuf}, result,
};

use hole_punch::{
    self, Config, ConfigBuilder, Context, CreateConnectionToPeerHandle, FileFormat, PubKeyHash,
};

use futures::{
    sync::oneshot, Async::{NotReady, Ready}, Future, Poll, Sink, Stream as FStream,
};

use tokio_core::reactor::{Core, Handle};

use openssl::pkey::{PKey, Private};

pub struct PeerBuilder {
    config: ConfigBuilder,
    handle: Handle,
    peer_context: PeerContext,
    private_key: Option<(FileFormat, Vec<u8>)>,
    private_key_file: Option<PathBuf>,
}

impl PeerBuilder {
    fn new(handle: Handle) -> PeerBuilder {
        let config = Config::builder();

        PeerBuilder {
            config,
            handle,
            peer_context: PeerContext::new(handle.clone()),
            private_key: None,
            private_key_file: None,
        }
    }

    /// Set the TLS certificate filename.
    pub fn set_cert_chain_file<C: Into<PathBuf>>(mut self, path: C) -> PeerBuilder {
        self.config.set_cert_chain_filename(path);
        self
    }

    /// Set the TLS private key filename.
    /// The key needs to be in `PEM` format.
    pub fn set_private_key_file<K: Into<PathBuf>>(mut self, path: K) -> PeerBuilder {
        let path = path.into();
        self.private_key_file = Some(path.clone());
        self.config.set_key_filename(path);
        self
    }

    /// Set the TLS certificate chain for this peer from memory.
    /// This will overwrite any prior call to `set_cert_chain_filename`.
    pub fn set_cert_chain(mut self, chain: Vec<Vec<u8>>, format: FileFormat) -> PeerBuilder {
        self.config.set_cert_chain(chain, format);
        self
    }

    /// Set the TLS private key for this peer from memory.
    /// This will overwrite any prior call to `set_private_key_filename`.
    pub fn set_private_key(mut self, key: Vec<u8>, format: FileFormat) -> PeerBuilder {
        self.private_key = Some((format, key.clone()));
        self.config.set_key(key, format);
        self
    }

    /// Register the given service at this peer.
    pub fn register_service<S: Server + 'static>(self, service: S) -> PeerBuilder {
        self.peer_context.register_service(service);
        self
    }

    /// Set the incoming CA certificate files.
    /// These CAs will be used to authenticate incoming connections.
    /// When these CAs are not given, all incoming connections will be authenticated successfully.
    pub fn set_client_ca_cert_files(mut self, files: Vec<PathBuf>) -> PeerBuilder {
        self.config.set_incoming_ca_certificates(files);
        self
    }

    /// Set the outgoing CA certificate files.
    /// These CAs will be used to authenticate outgoing connections.
    /// When these CAs are not given, all outgoing connections will be trusted.
    pub fn set_server_ca_cert_files(mut self, files: Vec<PathBuf>) -> PeerBuilder {
        self.config.set_outgoing_ca_certificates(files);
        self
    }

    /// Add remote peer.
    /// The peer will hold a connection to one of the given remote peers. If one connection is
    /// closed, a new connection to the next remote peer is created. This ensures that the local
    /// peer is reachable by other peers.
    pub fn add_remote_peer<T: ToSocketAddrs>(mut self, peer: T) -> Result<Self> {
        self.config.add_remote_peer(peer)?;
        Ok(self)
    }

    /// Builds the `Peer` instance.
    pub fn build(self) -> Result<Peer> {
        let private_key = self.load_private_key()?;
        let context = Context::new(
            PubKeyHash::from_private_key(private_key, true)?,
            self.handle.clone(),
            self.config.build()?,
        )?;
        Ok(Peer::new(self.handle, context, self.peer_context))
    }

    fn load_private_key(&self) -> Result<PKey<Private>> {
        if let Some((format, ref data)) = self.private_key {
            self.load_private_key_from_memory(format, data)
        } else if let Some(ref path) = self.private_key_file {
            self.load_private_key_from_file(path)
        } else {
            bail!("No private key given!")
        }
    }

    fn load_private_key_from_memory(
        &self,
        format: FileFormat,
        data: &[u8],
    ) -> Result<PKey<Private>> {
        match format {
            FileFormat::PEM => Ok(PKey::<Private>::private_key_from_pem(data)?),
            FileFormat::DER => Ok(PKey::<Private>::private_key_from_der(data)?),
        }
    }

    fn load_private_key_from_file(&self, path: &Path) -> Result<PKey<Private>> {
        let mut file = File::open(path)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        self.load_private_key_from_memory(FileFormat::PEM, &data)
    }
}

fn spawn_hole_punch_context(
    context: Context,
    peer_context: PeerContext,
    handle: &Handle,
) -> oneshot::Receiver<Result<()>> {
    let (sender, recv) = oneshot::channel();

    let inner_handle = handle.clone();
    handle.spawn(
        context
            .for_each(|stream| {
                inner_handle.spawn(
                    IncomingStream::new(stream, peer_context.clone())
                        .map_err(|e| println!("IncomingStream error: {:?}", e)),
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

pub struct Peer {
    handle: Handle,
    peer_context: PeerContext,
    context_result: oneshot::Receiver<Result<()>>,
    create_connection_to_peer_handle: CreateConnectionToPeerHandle,
}

impl Peer {
    fn new(handle: Handle, context: Context, peer_context: PeerContext) -> Peer {
        let create_connection_to_peer_handle = context.create_connection_to_peer_handle();

        let context_result = spawn_hole_punch_context(context, peer_context.clone(), &handle);

        Peer {
            handle,
            peer_context,
            context_result,
            create_connection_to_peer_handle,
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
        &self,
        service: S,
        peer: PubKeyHash,
    ) -> impl Future<Item = <S::Instance as Future>::Item, Error = S::Error>
    where
        S::Error: From<Error>,
    {
        let name = service.name();
        self.create_connection_to_peer_handle
            .create_connection_to_peer(peer)
            .map_err(|e| e.into())
            .and_then(|stream| {
                let stream = protocol_stream_create(stream);
                stream
                    .send(Protocol::RequestServiceStart { name: name.into() })
                    .and_then(|s| s.into_future().map_err(|e| e.0))
                    .map_err(|e| e.into())
            })
            .and_then(|(msg, stream)| match msg {
                None => bail!("Stream closed while requesting service!"),
                Some(Protocol::ServiceStarted { id }) => RunService::new(
                    service,
                    id,
                    stream.into(),
                    &self.handle,
                    &mut self.peer_context,
                ),
                Some(Protocol::ServiceNotFound) => bail!("Requested service({}) not found!", name),
                _ => bail!("Received not expected message!"),
            })
            .flatten()
    }
}

struct RunService<I, E> {
    instance: Box<dyn ServiceInstance<Item = I, Error = E>>,
    result_sender: Option<oneshot::Sender<result::Result<I, E>>>,
}

impl<I, E> RunService<I, E> {
    fn new<S: Client<Error = E>>(
        service: S,
        service_id: ServiceId,
        stream: Stream,
        handle: &Handle,
        peer_context: &mut PeerContext,
    ) -> result::Result<oneshot::Receiver<result::Result<I, E>>, E>
    where
        S::Error: From<Error>,
        S::Instance: Future<Item=I, Error=E> + 'static
    {
        let (result_sender, result_recv) = oneshot::channel();
        let stream = stream.into();
        let new_stream_handle = NewStreamHandle::new(service_id, &stream);
        let instance = service.start(handle, stream, new_stream_handle)?;

        let run_service = RunService {
            instance: Box::new(instance),
            result_sender: Some(result_sender),
        };

        Ok(result_recv)
    }
}

impl<Item, Error> ServiceInstance for RunService<Item, Error> {
    fn incoming_stream(&mut self, stream: Stream) {
        self.instance.incoming_stream(stream);
    }
}

impl<Item, Error> Future for RunService<Item, Error> {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.instance.poll() {
            _ => return Ok(NotReady),
        }
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

struct IncomingStream {
    stream: ProtocolStream,
    context: PeerContext,
}

impl IncomingStream {
    fn new(stream: hole_punch::Stream, context: PeerContext) -> IncomingStream {
        IncomingStream {
            stream: protocol_stream_create(stream),
            context,
        }
    }
}

impl Future for IncomingStream {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.stream.poll()) {
                None => return Ok(Ready(())),
                Some(Protocol::ConnectToService { id }) => {}
                Some(Protocol::RequestServiceStart { name }) => {}
                _ => bail!("Unexpected message at IncomingStream."),
            };
        }
    }
}
