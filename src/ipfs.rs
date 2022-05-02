use crate::chacha20::{DecryptionStream, EncryptionStream};
use crate::ed25519::*;
use crate::keys::Privkey;
use anyhow::Result;
use bytes::Bytes;
use cid::Cid;
use futures::Stream;
use futures::TryStreamExt;
use ipfs_api::{IpfsApi, IpfsClient};
use reqwest::Error;
use std::pin::Pin;
use std::str::FromStr;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;

/// Upload a stream of data to IPFS, encrypted with the volume's encryption key.
pub async fn upload_encrypt(
    ipfs: &IpfsClient,
    volume: &Privkey,
    data: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync>>,
) -> Result<Cid> {
    let stream = EncryptionStream::new(data, &volume.to_chacha20_key());
    let reader = stream.into_async_read();
    let cid = ipfs.add_async(reader).await?;
    let cid = Cid::from_str(&cid.hash)?;
    Ok(cid)
}

/// Fetch a snapshot from IPFS, decrypt it on-the-fly with the volume's decryption key.
pub async fn fetch_decrypt(
    ipfs: &IpfsClient,
    volume: &Privkey,
    cid: &Cid,
) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, ipfs_api::Error>> + Send>>, Error> {
    let data = ipfs.cat(&cid.to_string());
    let data = Box::pin(DecryptionStream::new(data, &volume.to_chacha20_key()));
    Ok(data)
}
