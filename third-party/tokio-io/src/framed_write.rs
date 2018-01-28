use std::io::{self, Read};
use std::fmt;

use {AsyncRead, AsyncWrite};
use codec::Decoder;
use framed::Fuse;

use futures::{Async, AsyncSink, Poll, Stream, Sink, StartSend};
use bytes::BytesMut;

macro_rules! mock {
    ($($x:expr,)*) => {{
        let mut v = VecDeque::new();
        v.extend(vec![$($x),*]);
        Mock { calls: v }
    }};
}

/// Trait of helper objects to write out messages as bytes, for use with
/// `FramedWrite`.
pub trait Encoder {
    /// The type of items consumed by the `Encoder`
    type Item;

    /// The type of encoding errors.
    ///
    /// `FramedWrite` requires `Encoder`s errors to implement `From<io::Error>`
    /// in the interest letting it return `Error`s directly.
    type Error: From<io::Error>;

    /// Encodes a frame into the buffer provided.
    ///
    /// This method will encode `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `Framed` instance and
    /// will be written out when possible.
    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut)
              -> Result<(), Self::Error>;
}

/// A `Sink` of frames encoded to an `AsyncWrite`.
pub struct FramedWrite<T, E> {
    inner: FramedWrite2<Fuse<T, E>>,
}

pub struct FramedWrite2<T> {
    inner: T,
    buffer: BytesMut,
}

const INITIAL_CAPACITY: usize = 8 * 1024;
const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;

impl<T, E> FramedWrite<T, E>
    where T: AsyncWrite,
          E: Encoder,
{
    /// Creates a new `FramedWrite` with the given `encoder`.
    pub fn new(inner: T, encoder: E) -> FramedWrite<T, E> {
        FramedWrite {
            inner: framed_write2(Fuse(inner, encoder)),
        }
    }
}

impl<T, E> FramedWrite<T, E> {
    /// Returns a reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        &self.inner.inner.0
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedWrite`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner.0
    }

    /// Consumes the `FramedWrite`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner.0
    }

    /// Returns a reference to the underlying decoder.
    pub fn encoder(&self) -> &E {
        &self.inner.inner.1
    }

    /// Returns a mutable reference to the underlying decoder.
    pub fn encoder_mut(&mut self) -> &mut E {
        &mut self.inner.inner.1
    }
}

impl<T, E> Sink for FramedWrite<T, E>
    where T: AsyncWrite,
          E: Encoder,
{
    type SinkItem = E::Item;
    type SinkError = E::Error;

    fn start_send(&mut self, item: E::Item) -> StartSend<E::Item, E::Error> {
        self.inner.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        Ok(try!(self.inner.close()))
    }
}

impl<T, D> Stream for FramedWrite<T, D>
    where T: Stream,
{
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.inner.inner.0.poll()
    }
}

impl<T, U> fmt::Debug for FramedWrite<T, U>
    where T: fmt::Debug,
          U: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FramedWrite")
         .field("inner", &self.inner.get_ref().0)
         .field("encoder", &self.inner.get_ref().1)
         .field("buffer", &self.inner.buffer)
         .finish()
    }
}

// ===== impl FramedWrite2 =====

pub fn framed_write2<T>(inner: T) -> FramedWrite2<T> {
    FramedWrite2 {
        inner: inner,
        buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
    }
}

impl<T> FramedWrite2<T> {
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Sink for FramedWrite2<T>
    where T: AsyncWrite + Encoder,
{
    type SinkItem = T::Item;
    type SinkError = T::Error;

    fn start_send(&mut self, item: T::Item) -> StartSend<T::Item, T::Error> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
            try!(self.poll_complete());

            if self.buffer.len() >= BACKPRESSURE_BOUNDARY {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        try!(self.inner.encode(item, &mut self.buffer));

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        trace!("flushing framed transport");

        while !self.buffer.is_empty() {
            trace!("writing; remaining={}", self.buffer.len());

            let n = try_nb!(self.inner.write(&self.buffer));

            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero, "failed to
                                          write frame to transport").into());
            }

            // TODO: Add a way to `bytes` to do this w/o returning the drained
            // data.
            let _ = self.buffer.split_to(n);
        }

        // Try flushing the underlying IO
        try_nb!(self.inner.flush());

        trace!("framed transport flushed");
        return Ok(Async::Ready(()));
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        try_ready!(self.poll_complete());
        Ok(try!(self.inner.shutdown()))
    }
}

impl<T: Decoder> Decoder for FramedWrite2<T> {
    type Item = T::Item;
    type Error = T::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<T::Item>, T::Error> {
        self.inner.decode(src)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<T::Item>, T::Error> {
        self.inner.decode_eof(src)
    }
}

impl<T: Read> Read for FramedWrite2<T> {
    fn read(&mut self, dst: &mut [u8]) -> io::Result<usize> {
        self.inner.read(dst)
    }
}

impl<T: AsyncRead> AsyncRead for FramedWrite2<T> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.inner.prepare_uninitialized_buffer(buf)
    }
}