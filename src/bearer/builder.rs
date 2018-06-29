use super::{ring::Ring, Bearer};
use error::*;

use std::{net::SocketAddr, path::PathBuf};

use hole_punch::{Config, FileFormat};

use tokio_core::reactor::Handle;

pub struct Builder {
    config: Config,
    handle: Handle,
    bearer_addr: SocketAddr,
    ring: Option<Ring>,
}

impl Builder {
    pub(crate) fn new(handle: &Handle, bearer_addr: SocketAddr) -> Builder {
        let mut config = Config::new();
        // we need the public key for verifying the proof
        config.enable_authenticator_store_orig_pub_key(true);

        Builder {
            config,
            handle: handle.clone(),
            bearer_addr,
            ring: None,
        }
    }

    /// Set the address where quic should listen on.
    /// This overwrites any prior call to `set_quic_listen_port`.
    pub fn set_quic_listen_address(mut self, address: SocketAddr) -> Builder {
        self.config.set_quic_listen_address(address);
        self
    }

    /// Set the port where quic should listen on (all interfaces).
    pub fn set_quic_listen_port(mut self, port: u16) -> Builder {
        self.config.set_quic_listen_port(port);
        self
    }

    /// Set the TLS certificate chain file (in PEM format).
    pub fn set_cert_chain_file<C: Into<PathBuf>>(mut self, path: C) -> Builder {
        self.config.set_cert_chain_filename(path);
        self
    }

    /// Set the TLS certificate chain.
    /// This will overwrite any prior call to `set_certificate_chain_file`.
    pub fn set_cert_chain(mut self, chain: Vec<Vec<u8>>, format: FileFormat) -> Builder {
        self.config.set_cert_chain(chain, format);
        self
    }

    /// Set the TLS private key file (in PEM format).
    pub fn set_private_key_file<P: Into<PathBuf>>(mut self, path: P) -> Builder {
        self.config.set_key_filename(path);
        self
    }

    /// Set the TLS private key.
    /// This will overwrite any prior call to `set_private_key_file`.
    pub fn set_private_key(mut self, key: Vec<u8>, format: FileFormat) -> Builder {
        self.config.set_key(key, format);
        self
    }

    /// Set the client CA certificate files.
    /// These CAs will be used to authenticate connecting clients.
    /// When these CAs are not given, all clients will be authenticated successfully.
    pub fn set_client_ca_cert_files(mut self, files: Vec<PathBuf>) -> Builder {
        self.config.set_client_ca_certificates(files);
        self
    }

    /// Let the bearer join the given Carrier Ring.
    pub fn join_ring(mut self, redis_host: SocketAddr, password: &str) -> Result<Builder> {
        let context = Ring::new(redis_host, password.into(), self.bearer_addr)?;
        self.ring = Some(context);
        Ok(self)
    }

    /// Build the `Bearer` instance.
    pub fn build(self) -> Result<Bearer> {
        if self.config.quic_config.cert_chain.is_none()
            && self.config.quic_config.cert_chain_filename.is_none()
        {
            bail!("The server requires a certificate.");
        }

        if self.config.quic_config.key.is_none() && self.config.quic_config.key_filename.is_none() {
            bail!("The server requires a private key.");
        }

        Bearer::new(self.handle, self.config, self.bearer_addr, self.ring)
    }
}
