use super::ServiceId;
use error::*;
use stream::Stream;

use futures::{
    sync::mpsc::{unbounded, Sender, UnboundedReceiver, UnboundedSender}, Async::Ready, Poll,
    Stream as FStream,
};

/// Returns the `Stream`s for a service instance.
/// It returns at least one `Stream`, the initial one.
/// Other `Stream`s are returned, when the other side of the service instance creates a new
/// `Stream` by using the `NewStreamHandle`.
pub struct Streams {
    first_stream: Option<Stream>,
    streams: UnboundedReceiver<Stream>,
    /// Send that the `Streams` instance is dropped.
    close_send: Sender<ServiceId>,
    /// The service id of the service instance this `Streams` instance belongs to.
    service_id: ServiceId,
}

impl Streams {
    pub(crate) fn new(
        first_stream: Stream,
        close_send: Sender<ServiceId>,
        service_id: ServiceId,
    ) -> (Streams, UnboundedSender<Stream>) {
        let (sender, streams) = unbounded();

        (
            Streams {
                first_stream: Some(first_stream),
                streams,
                close_send,
                service_id,
            },
            sender,
        )
    }
}

impl FStream for Streams {
    type Item = Stream;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.first_stream.take() {
            Some(stream) => Ok(Ready(Some(stream))),
            None => self
                .streams
                .poll()
                .map_err(|_| Error::from("Streams::poll() - unknown error")),
        }
    }
}

impl Drop for Streams {
    fn drop(&mut self) {
        let _ = self.close_send.try_send(self.service_id);
    }
}
