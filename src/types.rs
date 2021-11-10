use crate::ed25519::VerifyStream;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Bytes, BytesMut};
use futures::stream::{Stream, StreamExt};
use futures::task::Context;
use futures::task::Poll;
use reqwest::Error as ReqwestError;
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotHeader {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
}

impl SnapshotHeader {
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

    /*
    pub async fn header(&mut self) -> Result<SnapshotHeader, ReqwestError> {
        // i know, this really sucks, but not sure how to do this cleaner right now.
        if self.buffer.len() < SNAPSHOT_HEADER_SIZE {
            panic!("Not enough data for header: {}", self.buffer.len())
        }
        Ok(SnapshotHeader::from_bytes(&self.buffer).unwrap())
    }
    */
}

impl<E: StdError> Stream for HeaderStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let result = Pin::new(&mut self.stream).poll_next(cx);
        match &mut self.state {
            HeaderStreamState::Reading(buffer) => {
                match result {
                    Poll::Ready(Some(Ok(mut bytes))) => {
                        let total_bytes = buffer.len() + bytes.len();

                        // with the data we have buffered, is this enough to return some?
                        if total_bytes <= SNAPSHOT_HEADER_SIZE {
                            buffer.extend_from_slice(&bytes);
                            Poll::Ready(Some(Ok(Bytes::new())))
                        } else {
                            // split data into part we keep (part of the header) and the part
                            // that we return (any excess).
                            let data = bytes.split_off(SNAPSHOT_HEADER_SIZE - buffer.len());
                            buffer.extend_from_slice(&bytes);

                            // update state. we can safely call unwrap here, because we
                            // know that the size fits. if there was any other error
                            // error reason, we have to create our own error type and
                            // wrap E.
                            self.state = HeaderStreamState::Read(
                                SnapshotHeader::from_bytes(&buffer).unwrap(),
                            );
                            Poll::Ready(Some(Ok(bytes)))
                        }
                    }
                    result => result,
                }
            }
            HeaderStreamState::Read(_) => result,
        }
    }
}

pub struct BytesStreamBuffer<E: StdError> {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    buffer: BytesMut,
}

impl<E: StdError> BytesStreamBuffer<E> {
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static>(stream: S) -> Self {
        BytesStreamBuffer {
            stream: Box::pin(stream),
            buffer: BytesMut::new(),
        }
    }

    pub async fn buffer(&mut self) -> Result<Option<usize>, E> {
        match self.stream.next().await {
            Some(Ok(bytes)) => {
                self.buffer.extend_from_slice(&bytes);
                Ok(Some(bytes.len()))
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

impl<E: StdError> Stream for BytesStreamBuffer<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.buffer.len() > 0 {
            let bytes = self.buffer.clone().freeze();
            self.buffer.clear();
            Poll::Ready(Some(Ok(bytes)))
        } else {
            Pin::new(&mut self.stream).poll_next(cx)
        }
    }
}
