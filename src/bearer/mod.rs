use self::builder::Builder;
use self::connection::Connection;
use self::context::ContextPtr;
use error::*;
use protocol::Protocol;

use hole_punch::{Authenticator, Config, Context};

use futures::Async::Ready;
use futures::{Future, Poll, Stream as FStream};

use tokio_core::reactor::{Core, Handle};

mod builder;
mod connection;
mod context;

pub struct Bearer {
    context: Context<Protocol>,
    handle: Handle,
    server_context: ContextPtr,
    authenticator: Authenticator,
}

impl Bearer {
    fn new(handle: Handle, config: Config) -> Result<Bearer> {
        let context = Context::new(handle.clone(), config)?;

        let authenticator = match context.authenticator() {
            Some(auth) => auth,
            None => bail!("No authenticator was created!"),
        };

        let server_context = context::Context::new();

        Ok(Bearer {
            context,
            handle,
            server_context,
            authenticator,
        })
    }

    /// Creates a `BearerBuilder` for building a server instance.
    pub fn builder(handle: &Handle) -> Builder {
        Builder::new(handle)
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
