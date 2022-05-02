use crate::*;
use bytes::{Bytes, BytesMut};
use futures::stream::{self, TryStreamExt};
use ipfs_api::{IpfsClient, TryFromUri};
use std::ops::Deref;

fn ipfs_client() -> IpfsClient {
    let url = std::env::var("IPFS_API").unwrap();
    let ipfs_api = IpfsClient::from_str(&url).unwrap();
    ipfs_api
}

async fn test_ipfs_upload_data(ipfs_client: &IpfsClient, privkey: &Privkey, data: &[u8]) {
    let data_bytes = Bytes::copy_from_slice(&data);
    let stream = stream::iter(vec![Ok(data_bytes)]);
    let stream = Box::pin(stream);
    let cid = ipfs::upload_encrypt(&ipfs_client, &privkey, stream)
        .await
        .unwrap();
    let stream = ipfs::fetch_decrypt(&ipfs_client, &privkey, &cid)
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
    let ipfs_client = ipfs_client();
    test_ipfs_upload_data(&ipfs_client, &privkey, &[12, 21, 24, 102]).await;
    test_ipfs_upload_data(&ipfs_client, &privkey, &[42; 1024]).await;
    test_ipfs_upload_data(&ipfs_client, &privkey, &[123, 123, 123, 123, 123, 123]).await;
}
