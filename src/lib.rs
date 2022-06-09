//! Library used to interact with storage backend and IPFS (to store
//! encrypted snapshots and manage metadata).

pub use crate::chacha20::{DecryptionStream, EncryptionStream};
pub use crate::ipfs::*;
pub use crate::keys::{Hash, Privkey, Pubkey, Secret};
pub use crate::manifest::*;
pub use crate::types::*;
use anyhow::Result;
use bytes::Bytes;
use ed25519::*;
use futures::Stream;
use reqwest::{Body, Client};
use std::pin::Pin;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use url::Url;

pub mod chacha20;
pub mod ed25519;
mod ipfs;
pub mod keys;
mod manifest;
#[cfg(test)]
mod tests;
mod types;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Error making HTTP request: {0:}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Error parsing URL: {0:}")]
    UrlParse(#[from] url::ParseError),
    #[error("Error making HTTP request: {0:}")]
    Unsuccessful(reqwest::StatusCode),
    #[error("Other error occured: {0:?}")]
    Other(#[from] anyhow::Error),
}

/// Health check.
pub async fn health_check(api: &Url, client: &Client) -> Result<(), Error> {
    let url = api.join(&format!("/health"))?;
    let response = client.get(url).send().await?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(Error::Unsuccessful(response.status()))
    }
}

/// Fetch latest (as in, most current generation) based on the parent
/// generation that is passed.
pub async fn latest(
    api: &Url,
    client: &Client,
    volume: &Pubkey,
    parent: Option<u64>,
) -> Result<Option<SnapshotInfo>, Error> {
    let url = api
        .join(&format!("/api/v1/snapshot/{}/latest", &volume.to_hex()))
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
        .join(&format!("/api/v1/volume/{}/list", &volume.to_hex()))
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
pub async fn volume_create(
    api: &Url,
    client: &Client,
    token: &str,
    volume: &Privkey,
) -> Result<(), Error> {
    let url = api.join(&format!("/api/v1/volume/{}", &volume.pubkey().to_hex()))?;
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    Ok(())
}

/// Remove volume.
pub async fn volume_remove(
    api: &Url,
    client: &Client,
    token: &str,
    volume: &Privkey,
) -> Result<(), Error> {
    let url = api.join(&format!("/api/v1/volume/{}", &volume.pubkey().to_hex()))?;
    let response = client
        .delete(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    Ok(())
}

/// Upload a new snapshot
pub async fn snapshot_upload(
    api: &Url,
    client: &Client,
    token: &str,
    volume: &Privkey,
    manifest: &Manifest,
) -> Result<Hash, Error> {
    let url = api
        .join(&format!(
            "/api/v1/volume/{}/snapshot",
            &volume.pubkey().to_hex()
        ))
        .unwrap();
    let manifest = manifest.signed(volume);
    let hash = Manifest::hash(&manifest);
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {token}"))
        .body(manifest)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    Ok(hash)
}

/// Upload a new snapshot
pub async fn snapshot_fetch(
    api: &Url,
    client: &Client,
    token: &str,
    volume: &Privkey,
    snapshot: &Hash,
) -> Result<ManifestSigned, Error> {
    let url = api
        .join(&format!(
            "/api/v1/volume/{}/{}",
            &volume.pubkey().to_hex(),
            &snapshot.to_hex(),
        ))
        .unwrap();
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await?;
    let manifest = response.bytes().await?;
    let manifest = ManifestSigned::parse(&manifest)?;
    Ok(manifest)
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
        Pin<Box<dyn Stream<Item = Result<Bytes, VerifyError<reqwest::Error>>> + Send>>,
    ),
    reqwest::Error,
> {
    let url = api
        .join(&format!("/volume/{}/fetch", &volume.pubkey().to_hex()))
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
