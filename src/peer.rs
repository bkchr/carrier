use error::*;
use protocol::Protocol;

use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;
use std::net::SocketAddr;

use hole_punch::{plain, Config, Context, Stream};

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

struct Peer {
    handle: Handle,
    context: Context<Protocol>,
    peer_context: PeerContextPtr,
}

impl Peer {
    pub fn new<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        name: String,
    ) -> Result<Peer> {
        let config = Config {
            udp_listen_address: ([0, 0, 0, 0], 0).into(),
            cert_file: cert_file.into(),
            key_file: key_file.into(),
        };

        let context = Context::new(handle.clone(), config)?;

        Ok(Peer {
            context,
            handle: handle.clone(),
            peer_context: PeerContext::new(name),
        })
    }

    pub fn register_and_run(mut self, evt_loop: &mut Core, server: &SocketAddr) -> Result<()> {
        let peer_context = self.peer_context.clone();
        let name = peer_context.borrow().name.clone();
        let con = self.context
            .create_connection_to_server(server)
            .map_err(|e| e.into())
            .and_then(move |s| InitialConnection::register(s, name))
            .and_then(|s| Connection::new(s, peer_context));

        evt_loop.run(self.join(con)).map(|_| ()).map_err(|e| e.0)
    }

    pub fn login() {}
}

impl Future for Peer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match try_ready!(self.context.poll()) {
                Some(stream) => {}
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
                    let mut stream = self.stream.take().unwrap();
                    stream.upgrade_to_authenticated();
                    return Ok(Ready(stream));
                }
                _ => {}
            }
        }
    }
}
