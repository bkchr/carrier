use error::*;
use protocol::Protocol;
use service::ServiceId;

use hole_punch;

use futures::{Future, Poll, Sink, StartSend, Stream as FStream};

use tokio_io::{codec::length_delimited, AsyncRead, AsyncWrite};

use tokio_serde_json::{ReadJson, WriteJson};

use std::io::{self, Read, Write};

pub struct Stream {
    stream: hole_punch::Stream,
}

impl Stream {
    fn get_ref(&self) -> &hole_punch::Stream {
        &self.stream
    }
}

impl Into<Stream> for hole_punch::Stream {
    fn into(self) -> Stream {
        Stream { stream: self }
    }
}

impl FStream for Stream {
    type Item = <hole_punch::Stream as FStream>::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.stream.poll().map_err(|e| e.into())
    }
}

impl Sink for Stream {
    type SinkItem = <hole_punch::Stream as Sink>::SinkItem;
    type SinkError = Error;

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        self.stream.start_send(item).map_err(|e| e.into())
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.stream.poll_complete().map_err(|e| e.into())
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buf)
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stream.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Write::flush(&mut self.stream)
    }
}

impl AsyncRead for Stream {}

impl AsyncWrite for Stream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.stream.shutdown()
    }
}

pub type ProtocolStream = WriteJson<ReadJson<length_delimited::Framed<Stream>, Protocol>, Protocol>;

impl Into<Stream> for ProtocolStream {
    fn into(self) -> Stream {
        self.into_inner().into_inner().into_inner()
    }
}

pub fn protocol_stream_create(stream: hole_punch::Stream) -> ProtocolStream {
    WriteJson::new(ReadJson::new(length_delimited::Framed::new(stream.into())))
}

#[derive(Clone)]
pub struct NewStreamHandle {
    new_stream_handle: hole_punch::NewStreamHandle,
    service_id: ServiceId,
}

impl NewStreamHandle {
    pub(crate) fn new(service_id: ServiceId, stream: &Stream) -> NewStreamHandle {
        let new_stream_handle = stream.get_ref().new_stream_handle().clone();

        NewStreamHandle {
            new_stream_handle,
            service_id,
        }
    }

    pub fn new_stream(&self) -> impl Future<Item = Stream, Error = Error> {
        self.new_stream_handle
            .new_stream()
            .and_then(|stream| {
                stream
                    .send(Protocol::ConnectToService {
                        id: self.service_id,
                    })
                    .and_then(|s| s.into_future())
            })
            .and_then(|(msg, stream)| match msg {
                None => bail!("Stream closed!"),
                Some(Protocol::ServiceConnected) => stream.into(),
                Some(Protocol::ServiceNotFound) => bail!("Could not find requested service!"),
                _ => bail!("Received unexpected message!"),
            })
    }
}
