use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::task::Context;
use futures::task::Poll;
use futures::Stream;
use std::error::Error as StdError;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Atomic counter that is safe to be shared between threads, as it uses atomic
/// add and load operations.
#[derive(Clone, Debug)]
pub struct BytesCount {
    bytes: Arc<AtomicUsize>,
}

impl BytesCount {
    /// Creates new with initial value
    pub fn new(value: usize) -> Self {
        BytesCount {
            bytes: Arc::new(AtomicUsize::new(value)),
        }
    }

    /// Adds a value to the counter
    pub fn add(&self, value: usize) {
        self.bytes.fetch_add(value, Ordering::Relaxed);
    }

    /// Fetches the current value
    pub fn get(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }
}

/// Stream adaptor that has the ability to measure the amount of bytes that
/// pass through it.
pub struct CountBytesStream<E: StdError> {
    /// Underlying stream
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    /// Count of bytes that passed through so far
    count: BytesCount,
}

impl<E: StdError> CountBytesStream<E> {
    /// Create new stream from an underlying stream
    pub fn new(stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>) -> Self {
        CountBytesStream {
            stream,
            count: BytesCount::new(0),
        }
    }

    /// Return a clone of the BytesCount instance that can be used to fetch the number of bytes
    /// at a later point.
    pub fn bytes_count(&self) -> BytesCount {
        self.count.clone()
    }
}

impl<E: StdError> Stream for CountBytesStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                self.count.add(bytes.len());
                Poll::Ready(Some(Ok(bytes)))
            }
            other => other,
        }
    }
}

#[tokio::test]
async fn can_measure_bytes() {
    use futures::StreamExt;
    let data = Bytes::copy_from_slice(b"hello");
    let stream = futures::stream::iter(vec![Ok(data.clone())]);
    let mut stream = CountBytesStream::<std::io::Error>::new(Box::pin(stream));
    assert_eq!(stream.bytes_count().get(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data);
    assert_eq!(stream.bytes_count().get(), 5);

    let result = stream.next().await;
    assert!(result.is_none());
    assert_eq!(stream.bytes_count().get(), 5);
}

#[tokio::test]
async fn can_measure_bytes_multiple() {
    use futures::StreamExt;
    let data1 = Bytes::copy_from_slice(b"hello");
    let data2 = Bytes::copy_from_slice(b"world!");
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let mut stream = CountBytesStream::<std::io::Error>::new(Box::pin(stream));
    let count = stream.bytes_count();
    assert_eq!(count.get(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data1);
    assert_eq!(count.get(), 5);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data2);
    assert_eq!(count.get(), 11);

    let result = stream.next().await;
    assert!(result.is_none());
    assert_eq!(count.get(), 11);
}
