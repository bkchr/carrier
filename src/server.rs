use error::*;
use protocol::Protocol;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use std::net::SocketAddr;

use hole_punch::{Authenticator, Config, Context, PubKey, Stream, StreamHandle};

use futures::{Future, Poll, Stream as FStream};
use futures::Async::{NotReady, Ready};

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

pub struct Server {
    context: Context<Protocol>,
    handle: Handle,
    server_context: ServerContextPtr,
    authenticator: Authenticator,
}

impl Server {
    pub fn new<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        udp_listen_address: SocketAddr,
        trusted_client_certificates: Vec<PathBuf>,
    ) -> Result<Server> {
        let mut config = Config::new(udp_listen_address, cert_file.into(), key_file.into());

        config.set_trusted_client_certificates(trusted_client_certificates);

        let context = Context::new(handle.clone(), config)?;

        let authenticator = match context.authenticator() {
            Some(auth) => auth,
            None => bail!("No authenticator was created!"),
        };

        let server_context = ServerContext::new();

        Ok(Server {
            context,
            handle: handle.clone(),
            server_context,
            authenticator,
        })
    }

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
                Protocol::Hello => {
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
                            self.stream.send_and_poll(Protocol::Error(
                                "Could not find associated public key!".to_string(),
                            ))?;
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
