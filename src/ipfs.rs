use crate::keys::Secret;
use crate::stream::chacha20::{DecryptionStream, EncryptionStream};
use anyhow::Result;
use bytes::Bytes;
use cid::Cid;
use futures::{Stream, TryStreamExt};
use ipfs_api::{IpfsApi, IpfsClient};
use reqwest::Error;
use std::{pin::Pin, str::FromStr};

/// Upload a stream of data to IPFS, encrypted with the volume's encryption key.
pub async fn upload_encrypt(
    ipfs: &IpfsClient,
    secret: &Secret,
    data: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync>>,
) -> Result<Cid> {
    let stream = EncryptionStream::new(data, &secret.to_chacha20_key());
    let reader = stream.into_async_read();
    let cid = ipfs.add_async(reader).await?;
    let cid = Cid::from_str(&cid.hash)?;
    Ok(cid)
}

/// Fetch a snapshot from IPFS, decrypt it on-the-fly with the volume's decryption key.
pub async fn fetch_decrypt(
    ipfs: &IpfsClient,
    secret: &Secret,
    cid: &Cid,
) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, ipfs_api::Error>> + Send>>, Error> {
    let data = ipfs.cat(&cid.to_string());
    let data = Box::pin(DecryptionStream::new(data, &secret.to_chacha20_key()));
    Ok(data)
}
