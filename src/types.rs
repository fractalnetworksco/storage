use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Bytes, BytesMut};
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::io::Cursor;
use std::pin::Pin;

pub const SNAPSHOT_HEADER_SIZE: usize = 3 * 8;

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct SnapshotInfo {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SnapshotHeader {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
}

impl SnapshotHeader {
    pub fn new(generation: u64, parent: Option<u64>, creation: u64) -> Self {
        SnapshotHeader {
            generation,
            parent,
            creation,
        }
    }

    pub fn from_bytes(data: &[u8]) -> std::io::Result<Self> {
        let mut reader = Cursor::new(data);
        Ok(SnapshotHeader {
            generation: reader.read_u64::<BigEndian>()?,
            parent: match reader.read_u64::<BigEndian>()? {
                0 => None,
                value => Some(value),
            },
            creation: reader.read_u64::<BigEndian>()?,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.write_u64::<BigEndian>(self.generation).unwrap();
        data.write_u64::<BigEndian>(self.parent.unwrap_or(0))
            .unwrap();
        data.write_u64::<BigEndian>(self.creation).unwrap();
        data
    }

    pub fn to_info(&self, size: u64) -> SnapshotInfo {
        SnapshotInfo {
            generation: self.generation,
            parent: self.parent,
            size: size,
            creation: self.creation,
        }
    }
}

/// Represents the state of the HeaderStream.
enum HeaderStreamState {
    /// Initial state. We don't yet have enough data to read the header.
    Reading(BytesMut),
    /// Got the header, have some data buffered.
    Buffered(SnapshotHeader, Bytes),
    /// We have read the header.
    Read(SnapshotHeader),
}

/// Taking a bytes stream, decode the header, and pass through the other
/// data unchanged.
pub struct HeaderStream<E: StdError> {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    state: HeaderStreamState,
}

impl<E: StdError> HeaderStream<E> {
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static>(stream: S) -> Self {
        HeaderStream {
            stream: Box::pin(stream),
            state: HeaderStreamState::Reading(BytesMut::with_capacity(SNAPSHOT_HEADER_SIZE)),
        }
    }

    pub fn header(&self) -> Option<SnapshotHeader> {
        use HeaderStreamState::*;
        match &self.state {
            Buffered(header, _) => Some(header.clone()),
            Read(header) => Some(header.clone()),
            _ => None,
        }
    }
}

impl<E: StdError> Stream for HeaderStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use HeaderStreamState::*;
        // if there is some data buffered, return it
        match &mut self.state {
            Buffered(header, buffer) => {
                let buffer = buffer.clone();
                let header = header.clone();
                self.state = HeaderStreamState::Read(header);
                if buffer.len() > 0 {
                    return Poll::Ready(Some(Ok(buffer)));
                }
            }
            _ => {}
        }
        let result = Pin::new(&mut self.stream).poll_next(cx);
        match &mut self.state {
            Reading(buffer) => {
                match result {
                    Poll::Ready(Some(Ok(mut bytes))) => {
                        let total_bytes = buffer.len() + bytes.len();

                        // with the data we have buffered, is this enough to return some?
                        if total_bytes < SNAPSHOT_HEADER_SIZE {
                            buffer.extend_from_slice(&bytes);
                        } else {
                            // split data into part we keep (part of the header) and the part
                            // that we return (any excess).
                            let data = bytes.split_off(SNAPSHOT_HEADER_SIZE - buffer.len());
                            buffer.extend_from_slice(&bytes);

                            // update state. we can safely call unwrap here, because we
                            // know that the size fits. if there was any other error
                            // error reason, we have to create our own error type and
                            // wrap E.
                            self.state = HeaderStreamState::Buffered(
                                SnapshotHeader::from_bytes(&buffer).unwrap(),
                                data,
                            );
                        }
                        Poll::Ready(Some(Ok(Bytes::new())))
                    }
                    result => result,
                }
            }
            Read(_) => result,
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
#[tokio::test]
async fn header_only_parse() {
    use futures::StreamExt;
    let header = SnapshotHeader::new(1234, Some(1233), 128);
    let data: Bytes = header.to_bytes().into();
    let stream = futures::stream::iter(vec![Ok(data)]);
    let mut stream = HeaderStream::<std::io::Error>::new(stream);

    assert_eq!(stream.header(), None);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    // get right header
    assert_eq!(stream.header(), Some(header));

    // no data after
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn header_split_parse() {
    use futures::StreamExt;
    let header = SnapshotHeader::new(1234, Some(1233), 128);
    let data: Vec<Result<Bytes, std::io::Error>> = header
        .to_bytes()
        .into_iter()
        .map(|b| Ok(vec![b].into()))
        .collect();
    let stream = futures::stream::iter(data);
    let mut stream = HeaderStream::<std::io::Error>::new(stream);

    for _ in 0..SNAPSHOT_HEADER_SIZE {
        assert_eq!(stream.header(), None);
        let result = stream.next().await.unwrap();
        assert_eq!(result.unwrap().len(), 0);
    }

    // get right header
    assert_eq!(stream.header(), Some(header));

    // no data after
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn header_separate_parse() {
    use futures::StreamExt;
    let header = SnapshotHeader::new(1234, Some(1233), 128);
    let data1: Bytes = header.to_bytes().into();
    let data2: Bytes = "this is some test data".into();
    let stream = futures::stream::iter(vec![Ok(data1), Ok(data2.clone())]);
    let mut stream = HeaderStream::<std::io::Error>::new(stream);

    assert_eq!(stream.header(), None);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    // get right header
    assert_eq!(stream.header(), Some(header));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data2);

    // no data after
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn header_single_parse() {
    use futures::StreamExt;
    let header = SnapshotHeader::new(1234, Some(1233), 128);
    let mut data: BytesMut = header.to_bytes().as_slice().into();
    let text: Bytes = "this is some test data".into();
    data.extend_from_slice(&text);
    let stream = futures::stream::iter(vec![Ok(data.freeze())]);

    let mut stream = HeaderStream::<std::io::Error>::new(stream);

    assert_eq!(stream.header(), None);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    // get right header
    assert_eq!(stream.header(), Some(header));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), text);

    // no data after
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}
