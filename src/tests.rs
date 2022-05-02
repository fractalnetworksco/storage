use crate::*;
use ipfs_api::{IpfsClient, TryFromUri};

fn ipfs_client() -> IpfsClient {
    let url = std::env::var("IPFS_API").unwrap();
    let ipfs_api = IpfsClient::from_str(&url).unwrap();
    ipfs_api
}

async fn test_ipfs_upload_data(data: &u8) {}

#[tokio::test]
#[ignore]
async fn test_ipfs_upload() {
    let privkey = Privkey::generate();
    let ipfs_client = ipfs_client();
    let data = r#"some data"#;
}
