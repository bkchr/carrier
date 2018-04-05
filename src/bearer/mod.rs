use self::builder::Builder;
use self::context::ContextPtr;
use error::*;
use protocol::Protocol;

use std::net::SocketAddr;

use hole_punch::{Authenticator, Config, Context};

use futures::{Async::Ready, Future, Poll, Stream as FStream};

use tokio_core::reactor::{Core, Handle};

mod builder;
mod connection;
mod context;
mod ring;

/// A `Bearer` is on the Carrier Ring and holds connections to peers.
pub struct Bearer {
    /// The hole_punch context that will notify us about new connections.
    hp_context: Context<Protocol>,
    handle: Handle,
    context: ContextPtr,
    /// The `Authenticator` that authenticates peers.
    authenticator: Authenticator,
    /// The address, where this `Bearer` is reachable.
    bearer_addr: SocketAddr,
}

impl Bearer {
    fn new(
        handle: Handle,
        config: Config,
        bearer_addr: SocketAddr,
        ring: Option<ring::Ring>,
    ) -> Result<Bearer> {
        let hp_context = Context::new(handle.clone(), config)?;

        let authenticator = match hp_context.authenticator() {
            Some(auth) => auth,
            None => bail!("No authenticator was created!"),
        };

        let context = context::Context::new(ring);

        Ok(Bearer {
            hp_context,
            handle,
            context,
            authenticator,
            bearer_addr,
        })
    }

    /// Creates a `Builder` for building a `Bearer` instance.
    pub fn builder(handle: &Handle, bearer_addr: SocketAddr) -> Builder {
        Builder::new(handle, bearer_addr)
    }

    /// Run this `Bearer`.
    pub fn run(self, evt_loop: &mut Core) -> Result<()> {
        evt_loop.run(self)
    }
}

impl Future for Bearer {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let stream = match try_ready!(self.hp_context.poll()) {
                Some(stream) => stream,
                None => return Ok(Ready(())),
            };

            self.handle.spawn(
                connection::Initialize::new(
                    stream,
                    self.context.clone(),
                    self.authenticator.clone(),
                    self.bearer_addr,
                    self.handle.clone(),
                ).map_err(|e| println!("Connection initialization error: {:?}", e)),
            )
        }
    }
}
