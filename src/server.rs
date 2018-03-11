use error::*;
use protocol::Protocol;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::path::PathBuf;
use std::net::SocketAddr;

use hole_punch::{Config, Context, Stream, StreamHandle};

use futures::{Future, Poll, Stream as FStream};
use futures::Async::{NotReady, Ready};

use tokio_core::reactor::{Core, Handle};

struct ServerContext {
    devices: HashMap<String, StreamHandle<Protocol>>,
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
    fn register_connection(&mut self, name: &str, con: StreamHandle<Protocol>);

    fn unregister_connection(&mut self, name: &str);

    fn get_mut_connection(&mut self, name: &str) -> Option<StreamHandle<Protocol>>;
}

impl ServerContextTrait for ServerContextPtr {
    fn register_connection(&mut self, name: &str, con: StreamHandle<Protocol>) {
        if self.borrow_mut()
            .devices
            .insert(name.to_owned(), con)
            .is_some()
        {
            println!("overwriting known connection: {}", name);
        }
    }

    fn unregister_connection(&mut self, name: &str) {
        self.borrow_mut().devices.remove(name);
    }

    fn get_mut_connection(&mut self, name: &str) -> Option<StreamHandle<Protocol>> {
        self.borrow_mut().devices.get_mut(name).map(|v| v.clone())
    }
}

pub struct Server {
    context: Context<Protocol>,
    handle: Handle,
    server_context: ServerContextPtr,
}

impl Server {
    pub fn new<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        udp_listen_address: SocketAddr,
    ) -> Result<Server> {
        let config = Config::new(
            udp_listen_address,
            cert_file.into(),
            key_file.into(),
        );

        let context = Context::new(handle.clone(), config)?;

        let server_context = ServerContext::new();

        Ok(Server {
            context,
            handle: handle.clone(),
            server_context,
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
                Connection::new(stream, self.server_context.clone())
                    .map_err(|e| println!("{:?}", e)),
            )
        }
    }
}

struct Connection {
    stream: Stream<Protocol>,
    context: ServerContextPtr,
    name: Option<String>,
}

impl Connection {
    fn new(stream: Stream<Protocol>, context: ServerContextPtr) -> Connection {
        Connection {
            stream,
            context,
            name: None,
        }
    }

    fn poll_impl(&mut self) -> Poll<(), Error> {
        loop {
            let msg = match try_ready!(self.stream.poll()) {
                Some(msg) => msg,
                None => return Ok(Ready(())),
            };

            match msg {
                Protocol::Login { name, .. } => {
                    println!("Login: {}", name);
                    self.stream.send_and_poll(Protocol::LoginSuccessful)?;
                    self.stream.upgrade_to_authenticated();
                }
                Protocol::Register { name } => {
                    println!("Register: {}", name);
                    self.context
                        .register_connection(&name, self.stream.get_stream_handle());
                    self.name = Some(name);
                    self.stream.send_and_poll(Protocol::RegisterSuccessFul)?;
                    self.stream.upgrade_to_authenticated();
                }
                Protocol::ConnectToPeer {
                    name,
                    connection_id,
                } => {
                    if let Some(mut con) = self.context.get_mut_connection(&name) {
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
                if let Some(name) = self.name.take() {
                    self.context.unregister_connection(&name)
                };

                r
            }
        }
    }
}
