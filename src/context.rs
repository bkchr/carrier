use error::*;
use service::{Server, ServerResult, ServiceId, executor};
use stream::ProtocolStream;

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use tokio_core::reactor::Handle;

struct Inner {
    services: HashMap<String, Box<Server>>,
}

impl Inner {
    fn new() -> Inner {
        Inner {
            services: HashMap::new(),
        }
    }

    fn register_service<S: Server + 'static>(&mut self, service: S) {
        let name = service.name();
        self.services.insert(name, Box::new(service));
    }
}

#[derive(Clone)]
pub struct PeerContext {
    inner: Rc<RefCell<Inner>>,
    instances_executor: executor::InstancesExecutorHandle,
}

impl PeerContext {
    fn new(handle: Handle) -> PeerContext {
        let (instance_msg_sender, recv) = unbounded();
        PeerContext {
            inner: Rc::new(RefCell::new(Inner::new())),
            instance_msg_sender,
        }
    }

    pub fn register_service<S: Server + 'static>(&mut self, service: S) {
        self.inner.lock().unwrap().register_service(service);
    }

    pub fn add_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        inst: Box<ServerResult<Item = (), Error = Error>>,
    ) {
        let _ = self
            .instance_msg_sender
            .unbounded_send(ServiceInstancesExecutorMessage::NewInstance(inst_id, inst));
    }

    pub fn connect_stream_to_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        stream: ProtocolStream,
    ) {
        let _ = self
            .instance_msg_sender
            .unbounded_send(ServiceInstancesExecutorMessage::NewStream(inst_id, stream));
    }
}

