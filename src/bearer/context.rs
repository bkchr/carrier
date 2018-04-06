use super::ring::Ring;
use peer_proof::Proof;
use protocol::Protocol;

use hole_punch::{PubKey, StreamHandle};

use std::{cell::RefCell, collections::HashMap, net::SocketAddr, rc::Rc};

pub struct Context {
    devices: HashMap<PubKey, StreamHandle<Protocol>>,
    ring: Option<Ring>,
}

impl Context {
    pub fn new(ring: Option<Ring>) -> ContextPtr {
        Rc::new(RefCell::new(Context {
            devices: HashMap::new(),
            ring,
        }))
    }

    fn register_connection_impl(
        &mut self,
        pub_key: &PubKey,
        proof: Proof,
        con: StreamHandle<Protocol>,
    ) {
        self.devices.insert(pub_key.clone(), con);

        if let Some(ref mut ring) = self.ring {
            ring.broadcast_new_connection(pub_key, proof);
        }
    }

    fn find_connection_impl(&self, pub_key: &PubKey) -> FindResult {
        if let Some(handle) = self.devices.get(pub_key) {
            return FindResult::Local(handle.clone());
        } else if let Some(ref ring) = self.ring {
            if let Some(addr) = ring.find_connection(pub_key) {
                return FindResult::Remote(addr);
            }
        }

        FindResult::NotFound
    }
}

pub enum FindResult {
    Local(StreamHandle<Protocol>),
    Remote(SocketAddr),
    NotFound,
}

pub type ContextPtr = Rc<RefCell<Context>>;

pub trait ContextTrait {
    /// Register a connection at the context.
    fn register_connection(&mut self, pub_key: &PubKey, proof: Proof, con: StreamHandle<Protocol>);

    /// Unregister a connection, if the connection was closed.
    fn unregister_connection(&mut self, pub_key: &PubKey);

    /// Find a connection.
    /// If the `Bearer` is connected to the Carrier Ring, the location of the connection can be
    /// remote.
    fn find_connection(&self, pub_key: &PubKey) -> FindResult;
}

impl ContextTrait for ContextPtr {
    fn register_connection(&mut self, pub_key: &PubKey, proof: Proof, con: StreamHandle<Protocol>) {
        self.borrow_mut()
            .register_connection_impl(pub_key, proof, con);
    }

    fn unregister_connection(&mut self, pub_key: &PubKey) {
        self.borrow_mut().devices.remove(pub_key);
    }

    fn find_connection(&self, pub_key: &PubKey) -> FindResult {
        self.borrow().find_connection_impl(pub_key)
    }
}
