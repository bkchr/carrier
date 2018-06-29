use context::PeerContext;
use error::*;
use protocol::Protocol;
use service::{Client, Server};
use stream::ProtocolStream;

use std::fs::File;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::path::PathBuf;

use hole_punch::{
    self, Config, ConfigBuilder, Context, CreateConnectionToPeerHandle, FileFormat, PubKeyHash,
};

use futures::{sync::oneshot, Async::Ready, Future, Poll, Stream as FStream};

use tokio_core::reactor::{Core, Handle};

use openssl::pkey::{PKey, Private};

pub struct PeerBuilder {
    config: ConfigBuilder,
    handle: Handle,
    context: PeerContext,
}

impl PeerBuilder {
    fn new(handle: Handle) -> PeerBuilder {
        let config = Config::builder();

        PeerBuilder {
            config,
            handle,
            context: PeerContext::new(),
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
        self.config.set_key(key, format);
        self
    }

    /// Register the given service at this peer.
    pub fn register_service<S: Server + 'static>(self, service: S) -> PeerBuilder {
        let name = service.name();
        self.peer_context
            .borrow_mut()
            .services
            .insert(name.into(), Box::new(service));
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
        let peer_context = self.peer_context.clone();
        let peer_context2 = self.peer_context;
        let mut context = Context::new(self.handle.clone(), self.config)?;
        let handle = self.handle.clone();
        let handle2 = self.handle;
        let future = context
            .create_connection_to_server(&server)
            .map_err(|e| e.into())
            .and_then(move |s| {
                let mut con = Connection::new(s, peer_context, handle);
                con.send_hello(private_key, server)?;
                Ok(con)
            })
            .map(move |c| Peer::new(handle2, context, peer_context2, c));

        Ok(Peer::new())
    }

    fn load_private_key(&self) -> Result<PKey<Private>> {
        if let Some((format, ref data)) = self.config.quic_config.key {
            self.load_private_key_from_memory(format, data)
        } else if let Some(ref path) = self.config.quic_config.key_filename {
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
                let _ = sender.send(r);
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
            context,
            peer_context,
            context_result,
        }
    }

    /// Create a `PeerBuilder` for building a `Peer` instance.
    pub fn builder(handle: &Handle) -> PeerBuilder {
        PeerBuilder::new(handle)
    }

    /// Run this `Peer`.
    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }

    /// Connect to the given `Peer` and run the given `Service` (locally and remotely).
    pub fn run_service<S: Client>(
        self,
        service: S,
        peer: &PubKeyHash,
        handle: Handle,
    ) -> Result<impl Future<Item = S::Item, Error = S::Error>>
    where
        Error: From<S::Error>,
    {
        let name = service.name();
        self.context
            .create_connection_to_peer(peer)
            .and_then(|stream| {
                let stream: ProtocolStream = stream.into();
                stream
                    .send(Protocol::RequestServiceStart { name: name.into() })
                    .and_then(|s| s.into_future())
            })
            .and_then(|(msg, stream)| match msg {
                None => bail!("Stream closed while requesting service!"),
                Some(Protocol::ServiceStarted { id }) => {}
                Some(Protocol::ServiceNotFound) => bail!("Requested service({}) not found!", name),
                _ => bail!("Received not expected message!"),
            })
    }
}

impl Future for Peer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.context_result.poll()
    }
}

struct IncomingStream {
    stream: ProtocolStream,
    context: PeerContext,
}

impl IncomingStream {
    fn new(stream: hole_punch::Stream, context: PeerContext) -> IncomingStream {
        IncomingStream {
            stream: stream.into(),
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
                Protocol::ConnectToService { id } => {}
                Protocol::RequestServiceStart { id } => {}
                _ => bail!("Unexpected message at IncomingStream."),
            };
        }
    }
}
