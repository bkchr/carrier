use bearer::context::{ContextPtr, ContextTrait};
use error::*;
use hole_punch::{Authenticator, PubKey, Stream};
use protocol::Protocol;

use futures::{Async::{NotReady, Ready}, Future, Poll, Stream as FStream};

pub struct Connection {
    stream: Stream<Protocol>,
    context: ContextPtr,
    authenticator: Authenticator,
    pub_key: Option<PubKey>,
}

impl Connection {
    pub(crate) fn new(
        stream: Stream<Protocol>,
        context: ContextPtr,
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
                Protocol::Hello { proof } => {
                    self.pub_key = self.authenticator.client_pub_key(&self.stream);

                    match self.pub_key {
                        Some(ref key) => {
                            self.context.register_connection(
                                key,
                                proof,
                                self.stream.get_stream_handle(),
                            );
                            self.stream.upgrade_to_authenticated();
                            println!("Device registered: {}", key);
                        }
                        None => {
                            // should never happen, but how knows
                            self.stream.send_and_poll(Protocol::Error {
                                msg: "Could not find associated public key!".to_string(),
                            })?;
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
