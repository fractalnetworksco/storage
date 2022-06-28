use crate::*;
use bytes::Bytes;
use futures::stream::{self, TryStreamExt};
use ipfs_api::{IpfsClient, TryFromUri};
use rand_core::{OsRng, RngCore};
use std::ops::Deref;

/// Generate an IPFS client from the IPFS_API env variable.
fn ipfs_client() -> IpfsClient {
    let url = std::env::var("IPFS_API").unwrap();
    let ipfs_api = IpfsClient::from_str(&url).unwrap();
    ipfs_api
}

/// Given an IPFS client and a private key, upload some data to IPFS and then
/// check it, and finally make sure that what we got back matches what we sent.
async fn test_ipfs_upload_data(ipfs_client: &IpfsClient, secret: &Secret, data: &[u8]) {
    let data_bytes = Bytes::copy_from_slice(&data);
    let stream = stream::iter(vec![Ok(Bytes::new()), Ok(data_bytes), Ok(Bytes::new())]);
    let stream = Box::pin(stream);
    let cid = ipfs::upload_encrypt(&ipfs_client, &secret, stream)
        .await
        .unwrap();
    let stream = ipfs::fetch_decrypt(&ipfs_client, &secret, &cid)
        .await
        .unwrap();
    let stream_data: Vec<u8> = stream
        .map_ok(|v| v.deref().to_vec())
        .try_concat()
        .await
        .unwrap();
    assert_eq!(stream_data, data);
}

#[tokio::test]
#[ignore]
async fn test_ipfs_upload() {
    let privkey = Privkey::generate();
    let secret = privkey.derive_secret();
    let ipfs_client = ipfs_client();
    test_ipfs_upload_data(&ipfs_client, &secret, &[12, 21, 24, 102]).await;
    test_ipfs_upload_data(&ipfs_client, &secret, &[42; 1024]).await;
    test_ipfs_upload_data(&ipfs_client, &secret, &[123, 123, 123, 123, 123, 123]).await;
    test_ipfs_upload_data(&ipfs_client, &secret, &[104, 101, 108, 108, 111]).await;

    let mut data = vec![0; 1 * 1024 * 1024];
    OsRng.fill_bytes(&mut data[..]);
    test_ipfs_upload_data(&ipfs_client, &secret, &data).await;
}
