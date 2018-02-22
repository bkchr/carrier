use protocol::Protocol;

use std::path::PathBuf;
use std::rc::Rc;
use std::cell::RefCell;

use hole_punch::{plain, Config, Context, Stream};

use futures::{Future, Poll, Sink, Stream as FStream};
use futures::Async::Ready;

use failure::{Error, ResultExt};

use tokio_core::reactor::Handle;

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
}

impl Peer {
    pub fn new<S: Into<PathBuf>, T: Into<PathBuf>>(
        handle: &Handle,
        cert_file: S,
        key_file: T,
        name: String,
    ) -> Result<Peer, Error> {
        let config = Config {
            udp_listen_address: ([0, 0, 0, 0], 0).into(),
            cert_file: cert_file.into(),
            key_file: key_file.into(),
        };

        let context =
            Context::new(handle.clone(), config).context("error creating hole_punch context")?;

        Ok(Peer {
            context,
            handle: handle.clone(),
        })
    }
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
    stream: Option<Stream<Protocol>>,
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
    fn login(stream: Stream<Protocol>, name: String, password: String) -> InitialConnection {
        stream.start_send(Protocol::Login { name, password });
        stream.poll_complete();

        InitialConnection {
            stream: Some(stream),
        }
    }

    fn register(stream: Stream<Protocol>, name: String) -> InitialConnection {
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
                    return Ok(Ready(self.stream.take().unwrap()))
                }
                _ => {}
            }
        }
    }
}
