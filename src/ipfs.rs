use crate::chacha20::{DecryptionStream, EncryptionStream};
use crate::ed25519::*;
use crate::keys::{Privkey, Pubkey};
use crate::types::*;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use cid::Cid;
use futures::stream::IntoAsyncRead;
use futures::Stream;
use futures::TryStreamExt;
use ipfs_api::{IpfsApi, IpfsClient};
use reqwest::{Body, Client, Error};
use std::pin::Pin;
use std::str::FromStr;
use tokio::io::AsyncRead;
use tokio_stream::StreamExt;
use tokio_util::io::{ReaderStream, StreamReader};
use url::Url;

/// Upload a stream of data to IPFS, encrypted with the volume's encryption key.
pub async fn upload_encrypt(
    ipfs: &IpfsClient,
    volume: &Privkey,
    data: Pin<Box<dyn AsyncRead + Send + Sync>>,
) -> Result<Cid> {
    let data_stream = ReaderStream::new(data);
    let stream = EncryptionStream::new(data_stream, &volume.to_chacha20_key());
    let mut reader = stream.into_async_read();
    let cid = ipfs.add_async(reader).await?;
    let cid = Cid::from_str(&cid.hash)?;
    Ok(cid)
}

/// Fetch a snapshot from IPFS, decrypt it on-the-fly with the volume's decryption key.
pub async fn fetch_decrypt(
    ipfs: &IpfsClient,
    volume: &Privkey,
    hash: Option<Vec<u8>>,
    cid: &Cid,
) -> Result<Pin<Box<dyn Stream<Item = Bytes> + Sync + Send>>, Error> {
    let cid = "QmNrR8oEBeitvCqTugtH96JEo2wywBcqFb5bxsuhaBmML1";
    let data = ipfs.cat(&cid);
    unimplemented!()
}
