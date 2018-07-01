use protocol::Protocol;
use service::{Client, Server, ServiceId, Streams};
use stream::{NewStreamHandle, ProtocolStream, Stream};

use std::{cell::RefCell, collections::HashMap, rc::Rc, result};

use tokio_core::reactor::Handle;

use futures::{
    sync::mpsc::{channel, Receiver, Sender, UnboundedSender}, Sink, Stream as FStream,
};

struct Inner {
    services: HashMap<String, Box<Server>>,
    service_instances: HashMap<ServiceId, UnboundedSender<Stream>>,
    next_service_id: ServiceId,
    service_instance_dropped_sender: Sender<ServiceId>,
}

impl Inner {
    fn new() -> (Inner, Receiver<ServiceId>) {
        let (service_instance_dropped_sender, receiver) = channel(0);

        (
            Inner {
                services: HashMap::new(),
                service_instances: HashMap::new(),
                next_service_id: 0,
                service_instance_dropped_sender,
            },
            receiver,
        )
    }

    fn register_service<S: Server + 'static>(&mut self, service: S) {
        self.services
            .insert(service.name().into(), Box::new(service));
    }

    fn service_instance_dropped(&mut self, service_id: ServiceId) {
        self.service_instances.remove(&service_id);
    }

    fn next_service_id(&mut self) -> ServiceId {
        let res = self.next_service_id;
        self.next_service_id += 1;
        res
    }

    fn create_new_stream_handle_and_streams(
        &mut self,
        stream: Stream,
        local_service_id: ServiceId,
        remote_service_id: ServiceId,
    ) -> (NewStreamHandle, Streams) {
        let new_stream_handle = NewStreamHandle::new(remote_service_id, &stream);
        let (streams, streams_sender) = Streams::new(
            stream,
            self.service_instance_dropped_sender.clone(),
            local_service_id,
        );
        self.service_instances
            .insert(local_service_id, streams_sender);

        (new_stream_handle, streams)
    }

    fn start_server_service_instance(
        &mut self,
        name: &str,
        remote_service_id: ServiceId,
        mut stream: ProtocolStream,
        handle: &Handle,
    ) {
        if self.services.contains_key(name) {
            let id = self.next_service_id();
            send_protocol_message(&mut stream, Protocol::ServiceStarted { id });

            let (new_stream_handle, streams) =
                self.create_new_stream_handle_and_streams(stream.into(), id, remote_service_id);

            self.services
                .get_mut(name)
                .unwrap()
                .start(handle, streams, new_stream_handle);
        } else {
            send_protocol_message(&mut stream, Protocol::ServiceNotFound);
        }
    }

    fn start_client_service_instance<C>(
        &mut self,
        service: C,
        local_service_id: ServiceId,
        remote_service_id: ServiceId,
        stream: Stream,
        handle: &Handle,
    ) -> result::Result<C::Future, C::Error>
    where
        C: Client,
    {
        let (new_stream_handle, streams) =
            self.create_new_stream_handle_and_streams(stream, local_service_id, remote_service_id);

        service.start(handle, streams, new_stream_handle)
    }

    fn connect_stream_to_service_instance(
        &mut self,
        mut stream: ProtocolStream,
        service_id: ServiceId,
    ) {
        match self.service_instances.get_mut(&service_id) {
            Some(instance) => {
                send_protocol_message(&mut stream, Protocol::ServiceConnected);
                let _ = instance.unbounded_send(stream.into());
            }
            None => {
                send_protocol_message(&mut stream, Protocol::ServiceNotFound);
            }
        }
    }
}

fn send_protocol_message(stream: &mut ProtocolStream, msg: Protocol) {
    let _ = stream.start_send(msg);
    let _ = stream.poll_complete();
}

/// Spawn the service dropped receiver that informs the `PeerContext` about dropped service
/// instances.
fn spawn_service_dropped(
    mut context: PeerContext,
    service_dropped: Receiver<ServiceId>,
    handle: Handle,
) {
    handle.spawn(service_dropped.for_each(move |id| {
        context.service_instance_dropped(id);
        Ok(())
    }));
}

/// The context of the peer.
/// It stores all registered services and service instances.
#[derive(Clone)]
pub struct PeerContext {
    inner: Rc<RefCell<Inner>>,
}

impl PeerContext {
    pub fn new(handle: Handle) -> PeerContext {
        let (inner, service_dropped) = Inner::new();

        let context = PeerContext {
            inner: Rc::new(RefCell::new(inner)),
        };

        spawn_service_dropped(context.clone(), service_dropped, handle);

        context
    }

    pub fn register_service<S: Server + 'static>(&mut self, service: S) {
        self.inner.borrow_mut().register_service(service);
    }

    fn service_instance_dropped(&mut self, service_id: ServiceId) {
        self.inner.borrow_mut().service_instance_dropped(service_id);
    }

    pub fn start_server_service_instance(
        &mut self,
        name: &str,
        remote_service_id: ServiceId,
        stream: ProtocolStream,
        handle: &Handle,
    ) {
        self.inner.borrow_mut().start_server_service_instance(
            name,
            remote_service_id,
            stream,
            handle,
        );
    }

    pub fn start_client_service_instance<C>(
        &mut self,
        service: C,
        local_service_id: ServiceId,
        remote_service_id: ServiceId,
        stream: Stream,
        handle: &Handle,
    ) -> result::Result<C::Future, C::Error>
    where
        C: Client,
    {
        self.inner.borrow_mut().start_client_service_instance(
            service,
            local_service_id,
            remote_service_id,
            stream,
            handle,
        )
    }

    pub fn next_service_id(&mut self) -> ServiceId {
        self.inner.borrow_mut().next_service_id()
    }

    pub fn connect_stream_to_service_instance(
        &mut self,
        stream: ProtocolStream,
        service_id: ServiceId,
    ) {
        self.inner
            .borrow_mut()
            .connect_stream_to_service_instance(stream, service_id);
    }
}
