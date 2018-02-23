use error::*;
use protocol::Protocol;

use std::collections::HashMap;
use std::cell::RefCell;
use std::rc::Rc;

use hole_punch::{Context, Stream, StreamHandle};

use futures::{Future, Poll, Stream as FStream};
use futures::Async::Ready;

struct ServerContext {
    devices: HashMap<String, StreamHandle<Protocol>>,
}

type ServerContextPtr = Rc<RefCell<ServerContext>>;

struct Server {
    context: Context<Protocol>,
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
        }
    }
}

struct Connection {
    stream: Stream<Protocol>,
    context: ServerContextPtr,
}

impl Future for Connection {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        
    }
}
