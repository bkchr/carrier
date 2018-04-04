use error::*;
use protocol::Protocol;

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::rc::Rc;

use hole_punch::{Authenticator, Config, Context, FileFormat, PubKey, Stream, StreamHandle};

use futures::Async::{NotReady, Ready};
use futures::{Future, Poll, Stream as FStream};

use tokio_core::reactor::{Core, Handle};

struct ServerContext {
    devices: HashMap<PubKey, StreamHandle<Protocol>>,
}

impl ServerContext {
    fn new() -> ServerContextPtr {
        Rc::new(RefCell::new(ServerContext {
            devices: HashMap::new(),
        }))
    }
}

type ServerContextPtr = Rc<RefCell<ServerContext>>;

trait ServerContextTrait {
    fn register_connection(&mut self, pub_key: &PubKey, con: StreamHandle<Protocol>);

    fn unregister_connection(&mut self, pub_key: &PubKey);

    fn get_mut_connection(&mut self, pub_key: &PubKey) -> Option<StreamHandle<Protocol>>;
}

impl ServerContextTrait for ServerContextPtr {
    fn register_connection(&mut self, pub_key: &PubKey, con: StreamHandle<Protocol>) {
        if self.borrow_mut()
            .devices
            .insert(pub_key.clone(), con)
            .is_some()
        {
            println!("overwriting connection: {}", pub_key);
        }
    }

    fn unregister_connection(&mut self, pub_key: &PubKey) {
        self.borrow_mut().devices.remove(pub_key);
    }

    fn get_mut_connection(&mut self, pub_key: &PubKey) -> Option<StreamHandle<Protocol>> {
        self.borrow_mut()
            .devices
            .get_mut(pub_key)
            .map(|v| v.clone())
    }
}

pub struct ServerBuilder {
    config: Config,
    handle: Handle,
}

impl ServerBuilder {
    fn new(handle: &Handle) -> ServerBuilder {
        ServerBuilder {
            config: Config::new(),
            handle: handle.clone(),
        }
    }

    /// Set the address where quic should listen on.
    /// This overwrites any prior call to `set_quic_listen_port`.
    pub fn set_quic_listen_address(mut self, address: SocketAddr) -> ServerBuilder {
        self.config.set_quic_listen_address(address);
        self
    }

    /// Set the port where quic should listen on (all interfaces).
    pub fn set_quic_listen_port(mut self, port: u16) -> ServerBuilder {
        self.config.set_quic_listen_port(port);
        self
    }

    /// Set the TLS certificate chain file (in PEM format).
    pub fn set_cert_chain_file<C: Into<PathBuf>>(mut self, path: C) -> ServerBuilder {
        self.config.set_cert_chain_filename(path);
        self
    }

    /// Set the TLS certificate chain.
    /// This will overwrite any prior call to `set_certificate_chain_file`.
    pub fn set_cert_chain(mut self, chain: Vec<Vec<u8>>, format: FileFormat) -> ServerBuilder {
        self.config.set_cert_chain(chain, format);
        self
    }

    /// Set the TLS private key file (in PEM format).
    pub fn set_private_key_file<P: Into<PathBuf>>(mut self, path: P) -> ServerBuilder {
        self.config.set_key_filename(path);
        self
    }

    /// Set the TLS private key.
    /// This will overwrite any prior call to `set_private_key_file`.
    pub fn set_private_key(mut self, key: Vec<u8>, format: FileFormat) -> ServerBuilder {
        self.config.set_key(key, format);
        self
    }

    /// Set the client CA certificate files.
    /// These CAs will be used to authenticate connecting clients.
    /// When these CAs are not given, all clients will be authenticated successfully.
    pub fn set_client_ca_cert_files(mut self, files: Vec<PathBuf>) -> ServerBuilder {
        self.config.set_client_ca_certificates(files);
        self
    }

    /// Build the `Server` instance.
    pub fn build(self) -> Result<Server> {
        if self.config.quic_config.cert_chain.is_none()
            && self.config.quic_config.cert_chain_filename.is_none()
        {
            bail!("The server requires a certificate.");
        }

        if self.config.quic_config.key.is_none() && self.config.quic_config.key_filename.is_none() {
            bail!("The server requires a private key.");
        }

        Server::new(self.handle, self.config)
    }
}

pub struct Server {
    context: Context<Protocol>,
    handle: Handle,
    server_context: ServerContextPtr,
    authenticator: Authenticator,
}

impl Server {
    fn new(handle: Handle, config: Config) -> Result<Server> {
        let context = Context::new(handle.clone(), config)?;

        let authenticator = match context.authenticator() {
            Some(auth) => auth,
            None => bail!("No authenticator was created!"),
        };

        let server_context = ServerContext::new();

        Ok(Server {
            context,
            handle,
            server_context,
            authenticator,
        })
    }

    /// Creates a `ServerBuilder` for building a server instance.
    pub fn builder(handle: &Handle) -> ServerBuilder {
        ServerBuilder::new(handle)
    }

    /// Run this `Server`.
    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }
}

impl Future for Server {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let stream = match try_ready!(self.context.poll()) {
                Some(stream) => stream,
                None => return Ok(Ready(())),
            };

            self.handle.spawn(
                Connection::new(
                    stream,
                    self.server_context.clone(),
                    self.authenticator.clone(),
                ).map_err(|e| println!("{:?}", e)),
            )
        }
    }
}

struct Connection {
    stream: Stream<Protocol>,
    context: ServerContextPtr,
    authenticator: Authenticator,
    pub_key: Option<PubKey>,
}

impl Connection {
    fn new(
        stream: Stream<Protocol>,
        context: ServerContextPtr,
        authenticator: Authenticator,
    ) -> Connection {
        Connection {
            stream,
            context,
            authenticator,
            pub_key: None,
        }
    }

    fn poll_impl(&mut self) -> Poll<(), Error> {
        loop {
            let msg = match try_ready!(self.stream.poll()) {
                Some(msg) => msg,
                None => return Ok(Ready(())),
            };

            match msg {
                Protocol::Hello { .. } => {
                    self.pub_key = self.authenticator.client_pub_key(&self.stream);

                    match self.pub_key {
                        Some(ref key) => {
                            self.context
                                .register_connection(key, self.stream.get_stream_handle());
                            self.stream.upgrade_to_authenticated();
                            println!("Device registered: {}", key);
                        }
                        None => {
                            // should never happen, but how knows
                            self.stream.send_and_poll(Protocol::Error {
                                msg: "Could not find associated public key!".to_string(),
                            })?;
                            return Ok(Ready(()));
                        }
                    };
                }
                Protocol::ConnectToPeer {
                    pub_key,
                    connection_id,
                } => {
                    if let Some(mut con) = self.context.get_mut_connection(&pub_key) {
                        self.stream
                            .create_connection_to(connection_id, &mut con)
                            .unwrap();
                    } else {
                        self.stream.send_and_poll(Protocol::PeerNotFound)?;
                    }
                }
                _ => {}
            }
        }
    }
}

impl Future for Connection {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_impl() {
            Ok(NotReady) => {
                return Ok(NotReady);
            }
            r @ _ => {
                // If we got an error or `Ok(Ready(()))`, we unregister the connection
                if let Some(pub_key) = self.pub_key.take() {
                    self.context.unregister_connection(&pub_key)
                };

                r
            }
        }
    }
}
