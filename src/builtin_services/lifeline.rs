use error::*;
use service::{Client, Server, Streams};
use {NewStreamHandle, Stream};

use tokio::net::TcpStream;

use tokio::{
    self,
    io::{self, AsyncRead},
};

use tokio_file_unix;

use futures::{stream, Future, Poll, Sink, Stream as FStream};

use std::io::Write;

use bytes::Bytes;

pub struct Lifeline {}

impl Lifeline {
    pub fn new() -> Lifeline {
        Lifeline {}
    }
}

impl Server for Lifeline {
    fn start(&mut self, streams: Streams, _: NewStreamHandle) {
        tokio::spawn(
            streams
                .into_future()
                .map_err(|e| e.0)
                .and_then(move |(stream, _)| match stream {
                    Some(stream) => Ok(TcpStream::connect(&([127, 0, 0, 1], 22).into())
                        .map_err(|e| e.into())
                        .and_then(move |tcp| {
                            let (read, write) = AsyncRead::split(stream);
                            let (read2, write2) = tcp.split();

                            io::copy(read, write2)
                                .map(|_| ())
                                .select(io::copy(read2, write).map(|_| ()))
                                .map(|_| ())
                                .map_err(|e| Error::from(e.0))
                        })),
                    None => bail!("No `Stream` for Lifeline"),
                })
                .flatten()
                .map_err(|e| error!("Lifeline error: {:?}", e)),
        );
    }

    fn name(&self) -> &'static str {
        "lifeline"
    }
}

struct StdinReader<R: AsyncRead> {
    stdin: R,
    sink: stream::SplitSink<Stream>,
    buf: Vec<u8>,
}

impl<R: AsyncRead> StdinReader<R> {
    fn new(stdin: R, sink: stream::SplitSink<Stream>) -> StdinReader<R> {
        StdinReader {
            stdin,
            sink,
            buf: vec![0; 1024],
        }
    }
}

impl<R: AsyncRead> Future for StdinReader<R> {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let len = try_nb!(self.stdin.read(&mut self.buf));

            if len > 0 {
                self.sink.start_send(Bytes::from(&self.buf[..len]))?;
                self.sink.poll_complete()?;
            }
        }
    }
}

pub struct LifelineClientFuture {
    future: Box<Future<Item = (), Error = Error> + Send>,
}

impl LifelineClientFuture {
    fn new(streams: Streams) -> Result<LifelineClientFuture> {
        let stdin = tokio_file_unix::raw_stdin()?;
        let stdin = tokio_file_unix::File::new_nb(stdin)?;
        let stdin = stdin.into_reader(&tokio::reactor::Handle::default())?;

        let future = Box::new(
            streams
                .into_future()
                .map_err(|e| e.0)
                .and_then(|(stream, _)| match stream {
                    Some(stream) => {
                        let (sink, stream) = FStream::split(stream);

                        Ok(stream
                            .for_each(|buf| {
                                std::io::stdout().write(&buf)?;
                                std::io::stdout().flush()?;
                                Ok(())
                            })
                            .map_err(|e| e.into())
                            .join(StdinReader::new(stdin, sink).map_err(|e| e.into()))
                            .map(|_| ()))
                    }
                    None => bail!("No `Stream` for Lifeline"),
                })
                .flatten(),
        );

        Ok(LifelineClientFuture { future })
    }
}

impl Future for LifelineClientFuture {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll()
    }
}

impl Client for Lifeline {
    type Error = Error;
    type Future = LifelineClientFuture;

    fn start(self, streams: Streams, _: NewStreamHandle) -> Result<Self::Future> {
        LifelineClientFuture::new(streams)
    }

    fn name(&self) -> &'static str {
        "lifeline"
    }
}
