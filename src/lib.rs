pub mod ed25519;
mod types;

pub use crate::types::*;
use async_trait::async_trait;
use bytes::Bytes;
use ed25519::*;
use reqwest::{Body, Client, Error};
use std::pin::Pin;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;
use url::Url;

#[async_trait]
pub trait Storage {
    /// Fetch latest (as in, most current generation) based on the parent
    /// generation that is passed.
    async fn latest(
        &self,
        client: &Client,
        volume: &Pubkey,
        parent: Option<u64>,
    ) -> Result<Option<SnapshotInfo>, Error>;

    /// List snapshots, optionally restrict to ones with a given parent
    /// or a range limit on the generation.
    async fn list(
        &self,
        client: &Client,
        volume: &Pubkey,
        parent: Option<u64>,
        genmin: Option<u64>,
        genmax: Option<u64>,
    ) -> Result<Vec<SnapshotInfo>, Error>;

    /// Create new snapshot repository, given a private key.
    async fn create(&self, client: &Client, volume: &Privkey) -> Result<bool, Error>;

    /// Upload a new snapshot
    async fn upload(
        &self,
        client: &Client,
        volume: &Privkey,
        header: &SnapshotHeader,
        data: Pin<Box<dyn AsyncRead + Send + Sync>>,
    ) -> Result<Option<SnapshotInfo>, Error>;

    /// Fetch a snapshot from storage. This will decrypt and verify the
    /// signature on the snapshot, to make sure that it is valid and
    /// intact.
    async fn fetch(
        &self,
        client: &Client,
        volume: &Privkey,
        generation: u64,
        parent: Option<u64>,
    ) -> Result<HeaderVerifyStream, Error>;
}

#[async_trait]
impl Storage for Url {
    async fn latest(
        &self,
        client: &Client,
        volume: &Pubkey,
        parent: Option<u64>,
    ) -> Result<Option<SnapshotInfo>, Error> {
        let url = self
            .join(&format!("/snapshot/{}/latest", &volume.to_hex()))
            .unwrap();
        let mut query = vec![];
        if let Some(parent) = parent {
            query.push(("parent", parent.to_string()));
        }
        let response = client.get(url).query(&query).send().await?;
        Ok(response.json::<Option<SnapshotInfo>>().await?)
    }

    async fn list(
        &self,
        client: &Client,
        volume: &Pubkey,
        parent: Option<u64>,
        genmin: Option<u64>,
        genmax: Option<u64>,
    ) -> Result<Vec<SnapshotInfo>, Error> {
        let url = self
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

    async fn create(&self, client: &Client, volume: &Privkey) -> Result<bool, Error> {
        let url = self
            .join(&format!("/snapshot/{}/create", &volume.pubkey().to_hex()))
            .unwrap();
        let response = client.post(url).send().await?;
        Ok(response.status().is_success())
    }

    async fn upload(
        &self,
        client: &Client,
        volume: &Privkey,
        header: &SnapshotHeader,
        data: Pin<Box<dyn AsyncRead + Send + Sync>>,
    ) -> Result<Option<SnapshotInfo>, Error> {
        let url = self
            .join(&format!("/snapshot/{}/upload", &volume.pubkey().to_hex()))
            .unwrap();
        let header = header.to_bytes();
        let header_stream = tokio_stream::once(Ok(Bytes::from(header)));
        let stream = header_stream.chain(ReaderStream::new(data));
        let signed_stream = SignedStream::new(volume, Box::pin(stream));
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

    async fn fetch(
        &self,
        client: &Client,
        volume: &Privkey,
        generation: u64,
        parent: Option<u64>,
    ) -> Result<HeaderVerifyStream, Error> {
        let url = self
            .join(&format!("/snapshot/{}/fetch", &volume.pubkey().to_hex()))
            .unwrap();
        let mut query = vec![("generation", generation.to_string())];
        if let Some(parent) = parent {
            query.push(("parent", parent.to_string()));
        }
        let response = client.get(url).query(&query).send().await?;
        if response.status().is_success() {
            let stream = VerifyStream::new(&volume.pubkey(), Box::pin(response.bytes_stream()));
            let stream = HeaderVerifyStream::new(stream);
            Ok(stream)
        } else {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
