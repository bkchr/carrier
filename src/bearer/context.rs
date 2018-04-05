use super::ring::Ring;
use peer_proof::Proof;
use protocol::Protocol;

use hole_punch::{PubKey, StreamHandle};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
}

pub type ContextPtr = Rc<RefCell<Context>>;

pub trait ContextTrait {
    fn register_connection(&mut self, pub_key: &PubKey, proof: Proof, con: StreamHandle<Protocol>);

    fn unregister_connection(&mut self, pub_key: &PubKey);

    fn get_mut_connection(&mut self, pub_key: &PubKey) -> Option<StreamHandle<Protocol>>;
}

impl ContextTrait for ContextPtr {
    fn register_connection(&mut self, pub_key: &PubKey, proof: Proof, con: StreamHandle<Protocol>) {
        self.borrow_mut()
            .register_connection_impl(pub_key, proof, con);
    }

    fn unregister_connection(&mut self, pub_key: &PubKey) {
        self.borrow_mut().devices.remove(pub_key);
    }

    fn get_mut_connection(&mut self, pub_key: &PubKey) -> Option<StreamHandle<Protocol>> {
        self.borrow_mut()
            .devices
            .get_mut(pub_key)
            .map(|v| v.clone())
    }
}
