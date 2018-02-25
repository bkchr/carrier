use error::*;
use protocol::Protocol;
use service::Service;

use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::net::SocketAddr;
use std::collections::HashMap;

use hole_punch::{plain, Config, Context, Stream};

use futures::{Future, Poll, Sink, Stream as FStream};
use futures::Async::Ready;

use tokio_core::reactor::{Core, Handle};

type PeerContextPtr = Rc<RefCell<PeerContext>>;

struct PeerContext {
    name: String,
    services: HashMap<String, Box<Service>>,
}

impl PeerContext {
    fn new(name: String) -> PeerContextPtr {
        Rc::new(RefCell::new(PeerContext {
            name,
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

    pub fn register_service<S: Service + 'static>(&mut self, service: S) {
        let name = service.name();
        self.peer_context
            .borrow_mut()
            .services
            .insert(name, Box::new(service));
    }

    pub fn register(mut self, server: &SocketAddr) -> BuildPeer {
        let peer_context = self.peer_context.clone();
        let name = peer_context.borrow().name.clone();
        let handle = self.handle.clone();
        let future = self.context
            .create_connection_to_server(server)
            .map_err(|e| e.into())
            .and_then(move |s| InitialConnection::register(s, name))
            .map(move |s| {
                Peer::new(
                    self.handle,
                    self.context,
                    self.peer_context,
                    Connection::new(s, peer_context, handle),
                )
            });

        BuildPeer {
            future: Box::new(future),
        }
    }

    pub fn login(mut self, server: &SocketAddr, pw: String) -> BuildPeer {
        let peer_context = self.peer_context.clone();
        let name = peer_context.borrow().name.clone();
        let handle = self.handle.clone();
        let future = self.context
            .create_connection_to_server(server)
            .map_err(|e| e.into())
            .and_then(move |s| InitialConnection::login(s, name, pw))
            .map(move |s| {
                Peer::new(
                    self.handle,
                    self.context,
                    self.peer_context,
                    Connection::new(s, peer_context, handle),
                )
            });

        BuildPeer {
            future: Box::new(future),
        }
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
        name: String,
    ) -> Result<PeerBuilder> {
        PeerBuilder::new(handle, cert_file, key_file, name)
    }

    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }

    pub(crate) fn request_connection(
        &mut self,
        evt_loop: &mut Core,
        name: &str,
        service_name: &str,
    ) -> Result<plain::Stream> {
        let connection_id = self.context.generate_connection_id();
        let con = evt_loop.run(self.context.create_connection_to_peer(
            connection_id,
            self.server_con.stream.as_mut().unwrap(),
            Protocol::ConnectToPeer {
                name: name.into(),
                connection_id,
            },
        )?)?;

        evt_loop.run(RequestService::start(con, service_name)?)
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
            handle
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
                Protocol::RequestService { name } => {
                    if let Some(service) = self.context.borrow_mut().services.get_mut(&name) {
                        self.stream.as_mut().unwrap().send_and_poll(Protocol::ServiceConBuild)?;
                        service.spawn(&self.handle, self.stream.take().unwrap().into_plain())?;
                        return Ok(Ready(()));
                    } else {
                        self.stream.as_mut().unwrap().send_and_poll(Protocol::ServiceNotFound)?;
                    }
                }
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
