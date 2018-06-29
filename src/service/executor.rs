use super::{ServerResult, ServiceId};
use error::*;
use protocol::Protocol;
use stream::ProtocolStream;

use std::collections::HashMap;

use futures::{
    sync::mpsc::{unbounded, UnboundedReceiver, UnboundedSender}, Async::Ready, Future, Poll,
};

use tokio_core::reactor::Handle;

enum Message {
    NewInstance(ServiceId, Box<ServerResult<Item = (), Error = Error>>),
    NewStream(ServiceId, ProtocolStream),
}

pub struct InstancesExecutor {
    msg_recv: UnboundedReceiver<Message>,
    instances: HashMap<ServiceId, Box<ServerResult<Item = (), Error = Error>>>,
}

impl InstancesExecutor {
    pub fn new(handle: Handle) -> InstancesExecutorHandle {
        let (send, msg_recv) = unbounded();

        InstancesExecutor {
            msg_recv,
            instances: HashMap::new(),
        }
    }

    fn poll_msg_recv(&mut self) -> Poll<(), ()> {
        loop {
            let msg = match try_ready!(self.msg_recv.poll()) {
                None => {}
                Some(Message::NewInstance(id, instance)) => self.instances.insert(id, instance),
                Some(Message::NewStream(id, mut stream)) => {
                    if let Some(mut instance) = self.instances.get_mut(&id) {
                        let _ = stream.start_send(Protocol::ServiceConnected);
                        instance.new_stream(stream.into_inner().into_inner().into_inner());
                    } else {
                        let _ = stream.start_send(Protocol::ServiceNotFound);
                    }

                    let _ = stream.poll_complete();
                }
            };
        }
    }
}

impl Future for InstancesExecutor {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let _ = self.poll_msg_recv();

        self.instances.drain(|inst| match inst.poll() {
            Err(_) | Ok(Ready(())) => false,
            _ => true,
        });
    }
}

#[derive(Clone)]
pub struct InstancesExecutorHandle {
    msg_send: UnboundedSender<Message>,
}

impl InstancesExecutorHandle {
    fn new(msg_send: UnboundedSender<Message>) -> InstancesExecutorHandle {
        InstancesExecutorHandle { msg_send }
    }

    pub fn add_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        inst: Box<ServerResult<Item = (), Error = Error>>,
    ) {
        let _ = self
            .msg_send
            .unbounded_send(Message::NewInstance(inst_id, inst));
    }

    pub fn connect_stream_to_server_service_instance(
        &mut self,
        inst_id: ServiceId,
        stream: ProtocolStream,
    ) {
        let _ = self
            .msg_send
            .unbounded_send(Message::NewStream(inst_id, stream));
    }
}
