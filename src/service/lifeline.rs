use super::{Client, Server, Streams};
use error::*;
use {NewStreamHandle, Stream};

use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;

use tokio_io::io;
use tokio_io::AsyncRead;

use tokio_file_unix;

use futures::{stream, Future, Poll, Sink, Stream as FStream};

use std;
use std::io::Write;

use bytes::BytesMut;

lazy_static! {
    static ref LIFELINE_STDIN: std::io::Stdin = std::io::stdin();
}

pub struct Lifeline {}

impl Lifeline {
    pub fn new() -> Lifeline {
        Lifeline {}
    }
}

impl Server for Lifeline {
    fn start(&mut self, handle: &Handle, streams: Streams, _: NewStreamHandle) {
        let inner_handle = handle.clone();
        handle.spawn(
            streams
                .into_future()
                .map_err(|e| e.0)
                .and_then(move |(stream, _)| match stream {
                    Some(stream) => Ok(TcpStream::connect(
                        &([127, 0, 0, 1], 22).into(),
                        &inner_handle,
                    ).map_err(|e| e.into())
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
                .map_err(|e| println!("ERROR: {:?}", e)),
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
                self.sink.start_send(BytesMut::from(&self.buf[..len]))?;
                self.sink.poll_complete()?;
            }
        }
    }
}

pub struct LifelineClientFuture {
    future: Box<Future<Item = (), Error = Error>>,
}

impl LifelineClientFuture {
    fn new(handle: &Handle, streams: Streams) -> Result<LifelineClientFuture> {
        let lock = LIFELINE_STDIN.lock();
        let stdin = tokio_file_unix::StdFile(lock);
        let stdin = tokio_file_unix::File::new_nb(stdin)?;
        let stdin = stdin.into_reader(&handle)?;

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

    fn start(self, handle: &Handle, streams: Streams, _: NewStreamHandle) -> Result<Self::Future> {
        LifelineClientFuture::new(handle, streams)
    }

    fn name(&self) -> &'static str {
        "lifeline"
    }
}
