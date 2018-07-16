use context::PeerContext;
use error::*;
use peer::Peer;
use service::Server;

use std::{
    fs::File, io::Read, net::ToSocketAddrs, path::{Path, PathBuf},
};

use hole_punch::{Config, ConfigBuilder, Context, FileFormat, PubKeyHash};

use openssl::pkey::{PKey, Private};

use tokio_core::reactor::Handle;

pub struct PeerBuilder {
    config: ConfigBuilder,
    handle: Handle,
    peer_context: PeerContext,
    private_key: Option<(FileFormat, Vec<u8>)>,
    private_key_file: Option<PathBuf>,
}

impl PeerBuilder {
    pub(crate) fn new(handle: Handle) -> PeerBuilder {
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
        self.config = self.config.set_quic_listen_port(port);
        self
    }

    /// Set the TLS certificate chain filename.
    pub fn set_certificate_chain_file<C: Into<PathBuf>>(mut self, path: C) -> PeerBuilder {
        self.config = self.config.set_certificate_chain_filename(path);
        self
    }

    /// Set the TLS private key filename.
    /// The key needs to be in `PEM` format.
    pub fn set_private_key_file<K: Into<PathBuf>>(mut self, path: K) -> PeerBuilder {
        let path = path.into();
        self.private_key_file = Some(path.clone());
        self.config = self.config.set_private_key_filename(path);
        self
    }

    /// Set the TLS certificate chain for this peer from memory.
    /// This will overwrite any prior call to `set_cert_chain_filename`.
    pub fn set_certificate_chain(mut self, chain: Vec<Vec<u8>>, format: FileFormat) -> PeerBuilder {
        self.config = self.config.set_certificate_chain(chain, format);
        self
    }

    /// Set the TLS private key for this peer from memory.
    /// This will overwrite any prior call to `set_private_key_filename`.
    pub fn set_private_key(mut self, key: Vec<u8>, format: FileFormat) -> PeerBuilder {
        self.private_key = Some((format, key.clone()));
        self.config = self.config.set_private_key(key, format);
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
        self.config = self.config.set_incoming_ca_certificates(files);
        self
    }

    /// Set the outgoing CA certificate files.
    /// These CAs will be used to authenticate outgoing connections.
    /// When these CAs are not given, all outgoing connections will be trusted.
    pub fn set_server_ca_cert_files(mut self, files: Vec<PathBuf>) -> PeerBuilder {
        self.config = self.config.set_outgoing_ca_certificates(files);
        self
    }

    /// Add remote peer.
    /// The peer will hold a connection to one of the given remote peers. If one connection is
    /// closed, a new connection to the next remote peer is created. This ensures that the local
    /// peer is reachable by other peers.
    pub fn add_remote_peer<T: ToSocketAddrs>(mut self, peer: T) -> Result<Self> {
        self.config = self.config.add_remote_peer(peer)?;
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
