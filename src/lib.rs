pub mod chacha20;
pub mod ed25519;
mod ipfs;
pub mod keys;
mod manifest;
#[cfg(test)]
mod tests;
mod types;

use crate::chacha20::{DecryptionStream, EncryptionStream};
pub use crate::ipfs::*;
use crate::keys::{Privkey, Pubkey, Secret};
pub use crate::manifest::*;
pub use crate::types::*;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use ed25519::*;
use futures::Stream;
use reqwest::{Body, Client, Error};
use std::pin::Pin;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use url::Url;

/// Fetch latest (as in, most current generation) based on the parent
/// generation that is passed.
pub async fn latest(
    api: &Url,
    client: &Client,
    volume: &Pubkey,
    parent: Option<u64>,
) -> Result<Option<SnapshotInfo>, Error> {
    let url = api
        .join(&format!("/snapshot/{}/latest", &volume.to_hex()))
        .unwrap();
    let mut query = vec![];
    if let Some(parent) = parent {
        query.push(("parent", parent.to_string()));
    }
    let response = client.get(url).query(&query).send().await?;
    Ok(response.json::<Option<SnapshotInfo>>().await?)
}

/// List snapshots, optionally restrict to ones with a given parent
/// or a range limit on the generation.
pub async fn list(
    api: &Url,
    client: &Client,
    volume: &Pubkey,
    parent: Option<u64>,
    genmin: Option<u64>,
    genmax: Option<u64>,
) -> Result<Vec<SnapshotInfo>, Error> {
    let url = api
        .join(&format!("/snapshot/{}/list", &volume.to_hex()))
        .unwrap();
    let mut query = vec![];
    if let Some(parent) = parent {
        query.push(("parent", parent.to_string()));
    }
    if let Some(genmin) = genmin {
        query.push(("genmin", genmin.to_string()));
    }
    if let Some(genmax) = genmax {
        query.push(("genmax", genmax.to_string()));
    }
    let response = client.get(url).query(&query).send().await?;
    Ok(response.json::<Vec<SnapshotInfo>>().await?)
}

/// Create new snapshot repository, given a private key.
pub async fn create(api: &Url, client: &Client, volume: &Privkey) -> Result<bool, Error> {
    let url = api
        .join(&format!("/snapshot/{}/create", &volume.pubkey().to_hex()))
        .unwrap();
    let response = client.post(url).send().await?;
    Ok(response.status().is_success())
}

/// Upload a new snapshot
pub async fn upload(
    api: &Url,
    client: &Client,
    volume: &Privkey,
    header: &SnapshotHeader,
    data: Pin<Box<dyn AsyncRead + Send + Sync>>,
) -> Result<Option<SnapshotInfo>, Error> {
    let url = api
        .join(&format!("/snapshot/{}/upload", &volume.pubkey().to_hex()))
        .unwrap();
    let header = header.to_bytes();
    let header_stream = tokio_stream::once(Ok(Bytes::from(header)));
    let data_stream = ReaderStream::new(data);
    let stream = EncryptionStream::new(data_stream, &volume.to_chacha20_key());
    let stream = header_stream.chain(stream);
    let signed_stream = SignStream::new(stream, volume);
    let response = client
        .post(url)
        .body(Body::wrap_stream(signed_stream))
        .send()
        .await?;
    if response.status().is_success() {
        Ok(Some(response.json::<SnapshotInfo>().await?))
    } else {
        Ok(None)
    }
}

/// Fetch a snapshot from storage. This will decrypt and verify the
/// signature on the snapshot, to make sure that it is valid and
/// intact.
pub async fn fetch(
    api: &Url,
    client: &Client,
    volume: &Privkey,
    generation: u64,
    parent: Option<u64>,
) -> Result<
    (
        SnapshotHeader,
        Pin<Box<dyn Stream<Item = Result<Bytes, VerifyError<Error>>> + Send>>,
    ),
    Error,
> {
    let url = api
        .join(&format!("/snapshot/{}/fetch", &volume.pubkey().to_hex()))
        .unwrap();
    let mut query = vec![("generation", generation.to_string())];
    if let Some(parent) = parent {
        query.push(("parent", parent.to_string()));
    }
    let response = client.get(url).query(&query).send().await?;
    if response.status().is_success() {
        let stream = VerifyStream::new(&volume.pubkey(), response.bytes_stream());
        let mut stream = HeaderStream::new(stream);
        let header = loop {
            stream.next().await;
            if let Some(header) = stream.header() {
                break header;
            }
        };
        let stream = DecryptionStream::new(stream, &volume.to_chacha20_key());
        Ok((header, Box::pin(stream)))
    } else {
        unimplemented!()
    }
}
