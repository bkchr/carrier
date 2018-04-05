use peer_proof::Proof;
use protocol::Protocol;

use hole_punch::{PubKey, StreamHandle};

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Context {
    devices: HashMap<PubKey, StreamHandle<Protocol>>,
}

impl Context {
    pub fn new() -> ContextPtr {
        Rc::new(RefCell::new(Context {
            devices: HashMap::new(),
        }))
    }
}

pub type ContextPtr = Rc<RefCell<Context>>;

pub trait ContextTrait {
    fn register_connection(&mut self, pub_key: &PubKey, proof: Proof, con: StreamHandle<Protocol>);

    fn unregister_connection(&mut self, pub_key: &PubKey);

    fn get_mut_connection(&mut self, pub_key: &PubKey) -> Option<StreamHandle<Protocol>>;
}

impl ContextTrait for ContextPtr {
    fn register_connection(&mut self, pub_key: &PubKey, _: Proof, con: StreamHandle<Protocol>) {
        // TODO: store proof in redis
        if self.borrow_mut()
            .devices
            .insert(pub_key.clone(), con)
            .is_some()
        {
            println!("overwriting connection: {}", pub_key);
        }
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
