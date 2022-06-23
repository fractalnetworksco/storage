//! Library used to interact with storage backend and IPFS (to store
//! encrypted snapshots and manage metadata).

pub use crate::chacha20::{DecryptionStream, EncryptionStream};
pub use crate::ipfs::*;
pub use crate::keys::{Hash, Privkey, Pubkey, Secret};
pub use crate::manifest::*;
pub use crate::types::*;
use anyhow::Result;
use reqwest::Client;
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
pub async fn snapshot_list(
    api: &Url,
    client: &Client,
    token: &str,
    volume: &Pubkey,
    parent: Option<&Hash>,
    root: bool,
) -> Result<Vec<Hash>, Error> {
    let url = api
        .join(&format!("/api/v1/volume/{}/snapshots", &volume.to_hex()))
        .unwrap();
    let mut query = vec![];
    if let Some(parent) = parent {
        query.push(("parent", parent.to_string()));
    }
    if root {
        query.push(("root", "true".to_string()));
    }
    let response = client
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .query(&query)
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    Ok(response.json::<Vec<Hash>>().await?)
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
    volume: &Pubkey,
    manifest: &ManifestSigned,
) -> Result<(), Error> {
    let url = api
        .join(&format!("/api/v1/volume/{}/snapshot", &volume.to_hex()))
        .unwrap();
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {token}"))
        .body(manifest.data())
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    Ok(())
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
    if !response.status().is_success() {
        return Err(Error::Unsuccessful(response.status()));
    }
    let manifest = response.bytes().await?;
    let manifest = ManifestSigned::parse(&manifest)?;
    Ok(manifest)
}
