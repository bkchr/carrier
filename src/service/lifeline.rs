use error::*;
use super::Service;
use peer::BuildPeer;

use hole_punch::plain::Stream;

use tokio_core::reactor::{Core, Handle};
use tokio_core::net::TcpStream;

use tokio_io::io;
use tokio_io::AsyncRead;

use tokio_file_unix;

use futures::{stream, Future, Poll, Sink, Stream as FStream};

use std;
use std::io::Write;

use bytes::BytesMut;

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
                self.sink.start_send(BytesMut::from(&self.buf[..len]));
                self.sink.poll_complete();
            }
        }
    }
}

pub struct Lifeline {}

impl Service for Lifeline {
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

    fn run(self, evt_loop: &mut Core, peer: BuildPeer, name: &str) -> Result<()> {
        let mut peer = evt_loop.run(peer)?;
        let con = peer.request_connection(evt_loop, name, &self.name())?;
        let handle = evt_loop.handle();

        let stdin_orig = std::io::stdin();
        let stdin = tokio_file_unix::StdFile(stdin_orig.lock());
        let stdin = tokio_file_unix::File::new_nb(stdin).unwrap();
        let stdin = stdin.into_reader(&handle).unwrap();

        let (sink, stream) = FStream::split(con);

        handle.spawn(
            stream
                .for_each(|buf| {
                    std::io::stdout().write(&buf);
                    std::io::stdout().flush();
                    Ok(())
                })
                .map_err(|_| ()),
        );

        evt_loop.run(StdinReader::new(stdin, sink))?;
        Ok(())
    }

    fn name(&self) -> String {
        "lifeline".into()
    }
}
