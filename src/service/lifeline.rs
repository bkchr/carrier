use super::{Client, Server};
use error::*;

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
    fn spawn(&mut self, handle: &Handle, con: Stream) -> Result<()> {
        handle.spawn(
            TcpStream::connect(&([127, 0, 0, 1], 22).into(), &handle)
                .and_then(move |tcp| {
                    let (read, write) = AsyncRead::split(con);
                    let (read2, write2) = tcp.split();

                    io::copy(read, write2)
                        .map(|_| ())
                        .select(io::copy(read2, write).map(|_| ()))
                        .map(|_| ())
                        .map_err(|e| e.0)
                })
                .map_err(|e| println!("ERROR: {:?}", e)),
        );

        Ok(())
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
    type Error = std::io::Error;

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

impl Client for Lifeline {
    type Item = ();
    type Error = Error;
    type Future = Box<Future<Item = Self::Item, Error = Self::Error>>;

    fn start(self, handle: &Handle, con: Stream) -> Result<Self::Future> {
        let lock = LIFELINE_STDIN.lock();
        let stdin = tokio_file_unix::StdFile(lock);
        let stdin = tokio_file_unix::File::new_nb(stdin)?;
        let stdin = stdin.into_reader(&handle)?;

        let (sink, stream) = FStream::split(con);

        Ok(Box::new(
            stream
                .for_each(|buf| {
                    std::io::stdout().write(&buf)?;
                    std::io::stdout().flush()?;
                    Ok(())
                })
                .map_err(|e| e.into())
                .join(StdinReader::new(stdin, sink).map_err(|e| e.into()))
                .map(|_| ()),
        ))
    }

    fn name(&self) -> &'static str {
        "lifeline"
    }
}
