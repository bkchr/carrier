use error::*;
use service::{Server, ServiceInstance, ServiceId, executor};
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
        self.services.insert(service.name().into(), Box::new(service));
    }
}

/// The context of the peer.
/// It stores all registered services. Besides the services, the context is responsible for
/// executing instances of a service.
#[derive(Clone)]
pub struct PeerContext {
    inner: Rc<RefCell<Inner>>,
    instances_executor: executor::InstancesExecutorHandle,
}

impl PeerContext {
    pub fn new(handle: Handle) -> PeerContext {
        let instances_executor = executor::InstancesExecutor::new(handle);

        PeerContext {
            inner: Rc::new(RefCell::new(Inner::new())),
            instances_executor,
        }
    }

    pub fn register_service<S: Server + 'static>(&mut self, service: S) {
        self.inner.borrow_mut().register_service(service);
    }

    pub fn add_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        inst: Box<ServiceInstance<Item = (), Error = Error>>,
    ) {
        self.instances_executor.add_server_service_instance(inst_id, inst);
    }

    pub fn connect_stream_to_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        stream: ProtocolStream,
    ) {
        self.instances_executor.connect_stream_to_server_service_instance(inst_id, stream);
    }
}

