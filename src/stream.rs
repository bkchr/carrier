use error::*;
use protocol::Protocol;

use hole_punch;

use futures::{Poll, Sink, StartSend, Stream as FStream};

use tokio_io::{codec::length_delimited, AsyncRead, AsyncWrite};

use tokio_serde_json::{ReadJson, WriteJson};

use std::io::{self, Read, Write};

pub struct Stream {
    stream: hole_punch::Stream,
}

impl Stream {}

impl Into<Stream> for hole_punch::Stream {
    fn into(stream: hole_punch::Stream) -> Stream {
        Stream { stream }
    }
}

impl FStream for Stream {
    type Item = <hole_punch::Stream as FStream>::Item;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
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

impl Into<ProtocolStream> for hole_punch::Stream {
    fn into(stream: hole_punch::Stream) -> ProtocolStream {
        WriteJson::new(ReadJson::new(length_delimited::Framed::new(stream.into())))
    }
}

impl Into<Stream> for ProtocolStream {
    fn into(stream: ProtocolStream) -> Stream {
        stream.into_inner().into_inner().into_inner()
    }
}
