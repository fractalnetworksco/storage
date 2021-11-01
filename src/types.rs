use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use crate::ed25519::VerifyStream;
use reqwest::Error as ReqwestError;
use bytes::{Bytes, BytesMut};
use futures::stream::{Stream, StreamExt};
use futures::task::Context;
use futures::task::Poll;
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

pub struct HeaderVerifyStream {
    stream: VerifyStream<ReqwestError>,
    buffer: BytesMut,
    queue: Option<Bytes>,
}

impl HeaderVerifyStream {
    pub fn new(stream: VerifyStream<ReqwestError>) -> Self {
        HeaderVerifyStream {
            stream,
            buffer: BytesMut::with_capacity(SNAPSHOT_HEADER_SIZE),
            queue: None,
        }
    }

    pub async fn header(&mut self) -> Result<SnapshotHeader, ReqwestError> {
        // i know, this really sucks, but not sure how to do this cleaner right now.
        if self.buffer.len() < SNAPSHOT_HEADER_SIZE {
            panic!("Not enough data for header: {}", self.buffer.len())
        }
        Ok(SnapshotHeader::from_bytes(&self.buffer).unwrap())
    }

    pub fn verify(&self) -> Option<bool> {
        self.stream.verify()
    }
}

impl Stream for HeaderVerifyStream {
    type Item = Result<Bytes, ReqwestError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(bytes) = self.queue.clone() {
            self.queue = None;
            return Poll::Ready(Some(Ok(bytes)));
        }

        let result = Pin::new(&mut self.stream).poll_next(cx);

        if self.buffer.len() >= SNAPSHOT_HEADER_SIZE {
            return result;
        }

        match result {
            Poll::Ready(Some(Ok(mut bytes))) => {
                let total_bytes = self.buffer.len() + bytes.len();

                // with the data we have buffered, is this enough to return some?
                if total_bytes <= SNAPSHOT_HEADER_SIZE {
                    self.buffer.extend_from_slice(&bytes);
                } else {
                    // split data into part we keep (part of the header) and the part
                    // that we return (any excess).
                    let data = bytes.split_off(SNAPSHOT_HEADER_SIZE - self.buffer.len());
                    self.buffer.extend_from_slice(&bytes);
                    self.queue = Some(bytes);
                }
                Poll::Ready(Some(Ok(Bytes::new())))
            }
            result => result
        }
    }
}
