use hole_punch::ProtocolStream;
use protocol::Protocol;
use service::{Client, Server, ServiceId, Streams};
use stream::{NewStreamHandle, Stream};

use std::{
    collections::HashMap,
    result,
    sync::{Arc, Mutex},
};

use tokio::runtime::TaskExecutor;

use futures::{
    sync::mpsc::{channel, Receiver, Sender, UnboundedSender},
    Sink, Stream as FStream,
};

struct Inner {
    services: HashMap<String, Box<dyn Server + Send>>,
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
        mut stream: ProtocolStream<Protocol>,
    ) {
        if self.services.contains_key(name) {
            let id = self.next_service_id();
            send_protocol_message(&mut stream, Protocol::ServiceStarted { id });

            let (new_stream_handle, streams) =
                self.create_new_stream_handle_and_streams(stream.into(), id, remote_service_id);

            self.services
                .get_mut(name)
                .unwrap()
                .start(streams, new_stream_handle);
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
    ) -> result::Result<C::Future, C::Error>
    where
        C: Client,
    {
        let (new_stream_handle, streams) =
            self.create_new_stream_handle_and_streams(stream, local_service_id, remote_service_id);

        service.start(streams, new_stream_handle)
    }

    fn connect_stream_to_service_instance(
        &mut self,
        mut stream: ProtocolStream<Protocol>,
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

fn send_protocol_message(stream: &mut ProtocolStream<Protocol>, msg: Protocol) {
    let _ = stream.start_send(msg);
    let _ = stream.poll_complete();
}

/// Spawn the service dropped receiver that informs the `PeerContext` about dropped service
/// instances.
fn spawn_service_dropped(
    mut context: PeerContext,
    service_dropped: Receiver<ServiceId>,
    handle: TaskExecutor,
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
    inner: Arc<Mutex<Inner>>,
}

impl PeerContext {
    pub fn new(handle: TaskExecutor) -> PeerContext {
        let (inner, service_dropped) = Inner::new();

        let context = PeerContext {
            inner: Arc::new(Mutex::new(inner)),
        };

        spawn_service_dropped(context.clone(), service_dropped, handle);

        context
    }

    pub fn register_service<S: Server + 'static>(&mut self, service: S) {
        self.inner.lock().unwrap().register_service(service);
    }

    fn service_instance_dropped(&mut self, service_id: ServiceId) {
        self.inner
            .lock()
            .unwrap()
            .service_instance_dropped(service_id);
    }

    pub fn start_server_service_instance(
        &mut self,
        name: &str,
        remote_service_id: ServiceId,
        stream: ProtocolStream<Protocol>,
    ) {
        self.inner
            .lock()
            .unwrap()
            .start_server_service_instance(name, remote_service_id, stream);
    }

    pub fn start_client_service_instance<C>(
        &mut self,
        service: C,
        local_service_id: ServiceId,
        remote_service_id: ServiceId,
        stream: Stream,
    ) -> result::Result<C::Future, C::Error>
    where
        C: Client,
    {
        self.inner.lock().unwrap().start_client_service_instance(
            service,
            local_service_id,
            remote_service_id,
            stream,
        )
    }

    pub fn next_service_id(&mut self) -> ServiceId {
        self.inner.lock().unwrap().next_service_id()
    }

    pub fn connect_stream_to_service_instance(
        &mut self,
        stream: ProtocolStream<Protocol>,
        service_id: ServiceId,
    ) {
        self.inner
            .lock()
            .unwrap()
            .connect_stream_to_service_instance(stream, service_id);
    }
}
