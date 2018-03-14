use error::*;
use protocol::Protocol;
use service::{Client, Server};

use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::net::ToSocketAddrs;
use std::collections::HashMap;
use std::fmt::Display;

use hole_punch::{plain, Config, Context, PubKey, Stream};

use futures::{Future, Poll, Stream as FStream};
use futures::future::Either;
use futures::Async::Ready;

use tokio_core::reactor::{Core, Handle};

type PeerContextPtr = Rc<RefCell<PeerContext>>;

struct PeerContext {
    services: HashMap<String, Box<Server>>,
}

impl PeerContext {
    fn new() -> PeerContextPtr {
        Rc::new(RefCell::new(PeerContext {
            services: HashMap::new(),
        }))
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
        trusted_server_certificates: Vec<PathBuf>,
        trusted_client_certificates: Vec<PathBuf>,
    ) -> Result<PeerBuilder> {
        let mut config = Config::new(([0, 0, 0, 0], 0).into(), cert_file.into(), key_file.into());
        config.set_trusted_server_certificates(trusted_server_certificates);
        config.set_trusted_client_certificates(trusted_client_certificates);

        let context = Context::new(handle.clone(), config)?;

        Ok(PeerBuilder {
            context,
            handle: handle.clone(),
            peer_context: PeerContext::new(),
        })
    }

    pub fn register_service<S: Server + 'static>(&mut self, service: S) {
        let name = service.name();
        self.peer_context
            .borrow_mut()
            .services
            .insert(name.into(), Box::new(service));
    }

    pub fn connect<A: ToSocketAddrs + Display>(mut self, server: &A) -> Result<BuildPeer> {
        let server = match server.to_socket_addrs()?.nth(0) {
            Some(addr) => addr,
            None => bail!("Could not resolve any socket address from {}.", server),
        };

        let peer_context = self.peer_context.clone();
        let handle = self.handle.clone();
        let future = self.context
            .create_connection_to_server(&server)
            .map_err(|e| e.into())
            .and_then(move |s| {
                let mut con = Connection::new(s, peer_context, handle);
                con.send_hello()?;
                Ok(con)
            })
            .map(move |c| Peer::new(self.handle, self.context, self.peer_context, c));

        Ok(BuildPeer {
            future: Box::new(future),
        })
    }
}

pub struct BuildPeer {
    future: Box<Future<Item = Peer, Error = Error>>,
}

impl Future for BuildPeer {
    type Item = Peer;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll()
    }
}

pub struct Peer {
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

    pub fn build<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        trusted_server_certificates: Vec<PathBuf>,
        trusted_client_certificates: Vec<PathBuf>,
    ) -> Result<PeerBuilder> {
        PeerBuilder::new(
            handle,
            cert_file,
            key_file,
            trusted_server_certificates,
            trusted_client_certificates,
        )
    }

    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }

    pub fn run_service<S: Client>(
        mut self,
        evt_loop: &mut Core,
        service: S,
        peer: PubKey,
    ) -> Result<S::Item>
    where
        Error: From<S::Error>,
    {
        let connection_id = self.context.generate_connection_id();

        let con = self.context.create_connection_to_peer(
            connection_id,
            self.server_con.stream.as_mut().unwrap(),
            Protocol::ConnectToPeer {
                pub_key: peer,
                connection_id,
            },
        )?;

        let con = match evt_loop.run(con.select2(&mut self)).map_err(|e| match e {
            Either::A((e, _)) => e.into(),
            Either::B((e, _)) => e,
        })? {
            Either::A((con, _)) => con,
            Either::B(_) => {
                bail!("connection to server closed while waiting for connection to peer")
            }
        };

        let con = evt_loop.run(RequestService::start(con, service.name())?)?;

        evt_loop
            .run(service.start(&self.handle, con)?)
            .map_err(|e| e.into())
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
                    Connection::new(stream, self.peer_context.clone(), self.handle.clone())
                        .map_err(|e| eprintln!("{:?}", e)),
                ),
                None => return Ok(Ready(())),
            }
        }
    }
}

struct Connection {
    context: PeerContextPtr,
    handle: Handle,
    stream: Option<Stream<Protocol>>,
}

impl Connection {
    fn new(stream: Stream<Protocol>, context: PeerContextPtr, handle: Handle) -> Connection {
        Connection {
            stream: Some(stream),
            context,
            handle,
        }
    }

    fn send_hello(&mut self) -> Result<()> {
        self.stream
            .as_mut()
            .unwrap()
            .send_and_poll(Protocol::Hello)
            .map_err(|e| e.into())
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
                Protocol::RequestService { name } => {
                    if let Some(service) = self.context.borrow_mut().services.get_mut(&name) {
                        self.stream
                            .as_mut()
                            .unwrap()
                            .send_and_poll(Protocol::ServiceConBuild)?;
                        service.spawn(&self.handle, self.stream.take().unwrap().into_plain())?;
                        return Ok(Ready(()));
                    } else {
                        self.stream
                            .as_mut()
                            .unwrap()
                            .send_and_poll(Protocol::ServiceNotFound)?;
                    }
                }
                Protocol::PeerNotFound => {
                    panic!("Peer not found");
                }
                _ => {}
            }
        }
    }
}

struct RequestService {
    stream: Option<Stream<Protocol>>,
}

impl RequestService {
    fn start(mut stream: Stream<Protocol>, name: &str) -> Result<RequestService> {
        stream.send_and_poll(Protocol::RequestService { name: name.into() })?;
        Ok(RequestService {
            stream: Some(stream),
        })
    }
}

impl Future for RequestService {
    type Item = plain::Stream;
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
                None => bail!("connection closed while requesting service"),
            };

            match msg {
                Protocol::ServiceNotFound => bail!("service not found"),
                Protocol::ServiceConBuild => {
                    return Ok(Ready(self.stream.take().unwrap().into_plain()));
                }
                _ => {}
            }
        }
    }
}
