use bearer::context::{ContextPtr, ContextTrait, FindResult};
use error::*;
use hole_punch::{Authenticator, PubKey, Stream};
use peer_proof::{self, Proof};
use protocol::Protocol;

use std::net::SocketAddr;

use futures::{Async::{NotReady, Ready}, Future, Poll, Stream as FStream};

use tokio_core::reactor::Handle;

use openssl::pkey::{PKey, Public};

pub struct Connection {
    stream: Stream<Protocol>,
    context: ContextPtr,
    pub_key: PubKey,
}

impl Connection {
    fn new(stream: Stream<Protocol>, context: ContextPtr, pub_key: PubKey) -> Connection {
        Connection {
            stream,
            context,
            pub_key,
        }
    }

    fn poll_impl(&mut self) -> Poll<(), Error> {
        loop {
            let msg = match try_ready!(self.stream.poll()) {
                Some(msg) => msg,
                None => return Ok(Ready(())),
            };

            match msg {
                Protocol::ConnectToPeer {
                    pub_key,
                    connection_id,
                } => match self.context.find_connection(&pub_key) {
                    FindResult::Local(mut handle) => {
                        self.stream
                            .create_connection_to(connection_id, &mut handle)?;
                    }
                    FindResult::Remote(_) => unimplemented!(),
                    FindResult::NotFound => self.stream.send_and_poll(Protocol::PeerNotFound)?,
                },
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
            Ok(NotReady) => Ok(NotReady),
            r @ _ => {
                // If we got an error or `Ok(Ready(()))`, we unregister the connection
                self.context.unregister_connection(&self.pub_key);
                r
            }
        }
    }
}

/// Represents a connection that needs to be initialized.
/// It will resolve to a `Connection`, if the initialization could be finished successfully.
/// To initialize a `Connection`, it will wait for the `Protocol::Hello` message. After receiving
/// this message, the public key is extracted and the peer proof is verified. If the peer proof
/// could be verified successfully, a `Connection` instance is created.
pub struct Initialize {
    stream: Option<Stream<Protocol>>,
    context: ContextPtr,
    authenticator: Authenticator,
    bearer_address: SocketAddr,
    handle: Handle,
}

impl Initialize {
    /// Create a new instance.
    pub(crate) fn new(
        stream: Stream<Protocol>,
        context: ContextPtr,
        authenticator: Authenticator,
        bearer_address: SocketAddr,
        handle: Handle,
    ) -> Initialize {
        Initialize {
            stream: Some(stream),
            context,
            authenticator,
            bearer_address,
            handle,
        }
    }

    /// Check that the received message is `Protocol::Hello`.
    fn handle_message(&self, msg: Protocol) -> Result<Proof> {
        match msg {
            Protocol::Hello { proof } => Ok(proof),
            _ => bail!("Received unexpected message while waiting for initial message"),
        }
    }

    /// Extract the public key from the `Authenticator` and checks that the original public key
    /// is also available.
    fn extract_pubkey(&mut self) -> Result<(PubKey, PKey<Public>)> {
        let pub_key = self.authenticator
            .client_pub_key(self.stream.as_ref().unwrap());

        let pub_key = match pub_key {
            Some(key) => key,
            None => bail!("Could not find the public key for a connection"),
        };

        let orig_key = match pub_key.orig_public_key()? {
            Some(key) => key,
            None => bail!("`PubKey` does not contain the original public key"),
        };

        Ok((pub_key, orig_key))
    }

    fn poll_impl(&mut self) -> Poll<Connection, Error> {
        let msg = match try_ready!(
            self.stream
                .as_mut()
                .expect("Can not be polled twice")
                .poll()
        ) {
            Some(msg) => msg,
            None => bail!("Connection closed while waiting initial message"),
        };

        let proof = self.handle_message(msg)?;
        let (pkey, orig_pkey) = self.extract_pubkey()?;

        if peer_proof::verify_proof(&orig_pkey, &self.bearer_address, &proof)? {
            let mut stream = self.stream.take().unwrap();
            self.context
                .register_connection(&pkey, proof, stream.get_stream_handle());
            stream.upgrade_to_authenticated();
            println!("Peer registered: {}", pkey);

            Ok(Ready(Connection::new(stream, self.context.clone(), pkey)))
        } else {
            bail!("Proof verification failed")
        }
    }
}

impl Future for Initialize {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.poll_impl() {
            Ok(NotReady) => Ok(NotReady),
            Err(e) => {
                match self.stream {
                    Some(ref mut stream) => {
                        stream.send_and_poll(Protocol::Error {
                            msg: format!("{:?}", e),
                        })?;
                    }
                    None => {}
                }
                Err(e)
            }
            Ok(Ready(con)) => {
                self.handle
                    .spawn(con.map_err(|e| println!("Connection error: {:?}", e)));
                Ok(Ready(()))
            }
        }
    }
}
