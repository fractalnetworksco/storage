pub mod ed25519;
mod types;

pub use crate::types::*;
use reqwest::Client;
use url::Url;
use async_trait::async_trait;
use reqwest::Error;
use ed25519::*;

#[async_trait]
pub trait Storage {
    async fn latest(
        &self,
        client: &Client,
        volume: &Pubkey,
        parent: Option<u64>,
    ) -> Result<Option<SnapshotInfo>, Error>;
    async fn create(&self, client: &Client, volume: &Privkey) -> Result<bool, Error>;
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
        let response = client.get(url).send().await?;
        Ok(response.json::<Option<SnapshotInfo>>().await?)
    }

    async fn create(&self, client: &Client, volume: &Privkey) -> Result<bool, Error> {
        let url = self
            .join(&format!("/snapshot/{}/create", &volume.pubkey().to_hex()))
            .unwrap();
        let response = client.post(url).send().await.unwrap();
        Ok(response.status().is_success())
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
