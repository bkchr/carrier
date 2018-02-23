use error::*;
use protocol::Protocol;
use service::Service;

use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::net::SocketAddr;

use hole_punch::{Config, Context, Stream};

use futures::{Future, Poll, Sink, Stream as FStream};
use futures::Async::Ready;

use tokio_core::reactor::{Core, Handle};

type PeerContextPtr = Rc<RefCell<PeerContext>>;

struct PeerContext {
    name: String,
}

impl PeerContext {
    fn new(name: String) -> PeerContextPtr {
        Rc::new(RefCell::new(PeerContext { name }))
    }
}

pub struct PeerBuilder {
    context: Context<Protocol>,
    handle: Handle,
    peer_context: PeerContextPtr,
}

impl PeerBuilder {
    fn new<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        name: String,
    ) -> Result<PeerBuilder> {
        let config = Config {
            udp_listen_address: ([0, 0, 0, 0], 0).into(),
            cert_file: cert_file.into(),
            key_file: key_file.into(),
        };

        let context = Context::new(handle.clone(), config)?;

        Ok(PeerBuilder {
            context,
            handle: handle.clone(),
            peer_context: PeerContext::new(name),
        })
    }

    fn register(mut self, server: &SocketAddr) -> BuildPeer {
        let peer_context = self.peer_context.clone();
        let name = peer_context.borrow().name.clone();
        let future = self.context
            .create_connection_to_server(server)
            .map_err(|e| e.into())
            .and_then(move |s| InitialConnection::register(s, name))
            .map(move |s| {
                Peer::new(
                    self.handle,
                    self.context,
                    self.peer_context,
                    Connection::new(s, peer_context),
                )
            });

        BuildPeer {
            future: Box::new(future),
        }
    }

    fn login(mut self, server: &SocketAddr, pw: String) -> BuildPeer {
        let peer_context = self.peer_context.clone();
        let name = peer_context.borrow().name.clone();
        let future = self.context
            .create_connection_to_server(server)
            .map_err(|e| e.into())
            .and_then(move |s| InitialConnection::login(s, name, pw))
            .map(move |s| {
                Peer::new(
                    self.handle,
                    self.context,
                    self.peer_context,
                    Connection::new(s, peer_context),
                )
            });

        BuildPeer {
            future: Box::new(future),
        }
    }
}

struct BuildPeer {
    future: Box<Future<Item = Peer, Error = Error>>,
}

impl Future for BuildPeer {
    type Item = Peer;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll()
    }
}

struct Peer {
    handle: Handle,
    context: Context<Protocol>,
    peer_context: PeerContextPtr,
    server_con: Connection,
}

impl Peer {
    fn new(
        handle: Handle,
        context: Context<Protocol>,
        peer_context: PeerContextPtr,
        server_con: Connection,
    ) -> Peer {
        Peer {
            handle,
            context,
            peer_context,
            server_con,
        }
    }

    pub fn builder<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        name: String,
    ) -> Result<PeerBuilder> {
        PeerBuilder::new(handle, cert_file, key_file, name)
    }

    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }

    pub fn request_connection_and_run_service<S: Service>(
        self,
        evt_loop: &mut Core,
        name: String,
        service: S,
    ) -> Result<()> {
        unimplemented!()
    }
}

impl Future for Peer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if self.server_con.poll()?.is_ready() {
            return Ok(Ready(()));
        }

        loop {
            match try_ready!(self.context.poll()) {
                Some(stream) => self.handle.spawn(
                    Connection::new(stream, self.peer_context.clone())
                        .map_err(|e| eprintln!("{:?}", e)),
                ),
                None => return Ok(Ready(())),
            }
        }
    }
}

struct Connection {
    context: PeerContextPtr,
    stream: Option<Stream<Protocol>>,
}

impl Connection {
    fn new(stream: Stream<Protocol>, context: PeerContextPtr) -> Connection {
        Connection {
            stream: Some(stream),
            context,
        }
    }
}

impl Future for Connection {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let msg = match try_ready!(
                self.stream
                    .as_mut()
                    .expect("can not be polled twice")
                    .poll()
            ) {
                Some(msg) => msg,
                // connection/stream was closed, so we close this connection as well.
                None => return Ok(Ready(())),
            };

            match msg {
                _ => {}
            }
        }
    }
}

struct InitialConnection {
    stream: Option<Stream<Protocol>>,
}

impl InitialConnection {
    fn login(mut stream: Stream<Protocol>, name: String, password: String) -> InitialConnection {
        stream.start_send(Protocol::Login { name, password });
        stream.poll_complete();

        InitialConnection {
            stream: Some(stream),
        }
    }

    fn register(mut stream: Stream<Protocol>, name: String) -> InitialConnection {
        stream.start_send(Protocol::Register { name });
        stream.poll_complete();

        InitialConnection {
            stream: Some(stream),
        }
    }
}

impl Future for InitialConnection {
    type Item = Stream<Protocol>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let msg = match try_ready!(
                self.stream
                    .as_mut()
                    .expect("can not be polled twice")
                    .poll()
            ) {
                Some(msg) => msg,
                None => bail!("connection closed while waiting for successful registration/login"),
            };

            match msg {
                Protocol::RegisterSuccessFul | Protocol::LoginSuccessful => {
                    println!("Registration/Login successful");
                    let mut stream = self.stream.take().unwrap();
                    stream.upgrade_to_authenticated();
                    return Ok(Ready(stream));
                }
                _ => {}
            }
        }
    }
}
