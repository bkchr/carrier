use context::PeerContext;
use error::*;
use protocol::Protocol;
use service::{Client, Server};
use stream::{protocol_stream_create, ProtocolStream};

use std::{
    fs::File, io::Read, net::SocketAddr, net::ToSocketAddrs, path::{Path, PathBuf},
};

use hole_punch::{
    Config, ConfigBuilder, Context, CreateConnectionToPeerHandle, FileFormat, PubKeyHash,
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
        let peer_context = PeerContext::new(handle.clone());

        PeerBuilder {
            config,
            handle,
            peer_context,
            private_key: None,
            private_key_file: None,
        }
    }

    /// Set Quic listen port.
    pub fn set_quic_listen_port(mut self, port: u16) -> PeerBuilder {
        self.config.set_quic_listen_port(port);
        self
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
    pub fn register_service<S: Server + 'static>(mut self, service: S) -> PeerBuilder {
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

pub struct Peer {
    handle: Handle,
    peer_context: PeerContext,
    context_result: oneshot::Receiver<Result<()>>,
    create_connection_to_peer_handle: CreateConnectionToPeerHandle,
    quic_local_addr: SocketAddr,
}

impl Peer {
    fn new(handle: Handle, context: Context, peer_context: PeerContext) -> Peer {
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
