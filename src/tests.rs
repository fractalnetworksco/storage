use crate::volume::Volume;
use crate::Options;
use anyhow::Result;
use fractal_storage_client::*;
use rand::{thread_rng, Rng};
use reqwest::Client;
use reqwest::StatusCode;
use sqlx::AnyPool;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const WAIT_UP_TIMEOUT: Duration = Duration::from_secs(2);
const PORT_RANGE: Range<u16> = 50000..60000;

async fn temp_database() -> Result<AnyPool, sqlx::Error> {
    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    Ok(pool)
}

#[tokio::test]
async fn test_migrations() {
    let _pool = temp_database().await.unwrap();
}

#[tokio::test]
async fn test_volume_create() {
    let pool = temp_database().await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let account = Uuid::new_v4();
    let volume_privkey = Privkey::generate();
    let volume_pubkey = volume_privkey.pubkey();

    // does not exist yet
    let volume = Volume::lookup(&mut conn, &volume_pubkey).await.unwrap();
    assert!(volume.is_none());

    // create volume
    Volume::create(&mut conn, &volume_pubkey, &account)
        .await
        .unwrap();

    // check it's all there.
    let volume = Volume::lookup(&mut conn, &volume_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(volume.pubkey(), &volume_pubkey);
    assert_eq!(volume.account(), &account);
}

#[tokio::test]
async fn test_volume_edit() {
    let pool = temp_database().await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let account = Uuid::new_v4();
    let volume_privkey = Privkey::generate();
    let volume_pubkey = volume_privkey.pubkey();

    // create volume
    Volume::create(&mut conn, &volume_pubkey, &account)
        .await
        .unwrap();

    // check it's all there.
    let volume = Volume::lookup(&mut conn, &volume_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(volume.pubkey(), &volume_pubkey);
    assert_eq!(volume.account(), &account);

    // change account, set writer, make locked
    let new_account = Uuid::new_v4();
    let writer = Uuid::new_v4();
    volume
        .volume()
        .writer_set(&mut conn, Some(&writer))
        .await
        .unwrap();
    volume.volume().locked_set(&mut conn, true).await.unwrap();
    volume
        .volume()
        .account_set(&mut conn, &new_account)
        .await
        .unwrap();

    let volume = Volume::lookup(&mut conn, &volume_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(volume.account(), &new_account);
    assert_eq!(volume.locked(), true);
    assert_eq!(volume.writer(), Some(&writer));
}

#[tokio::test]
async fn test_snapshot_upload() {
    let pool = temp_database().await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let account = Uuid::new_v4();
    let volume_privkey = Privkey::generate();
    let volume_pubkey = volume_privkey.pubkey();

    // create volume
    Volume::create(&mut conn, &volume_pubkey, &account)
        .await
        .unwrap();

    // check it's all there.
    let volume = Volume::lookup(&mut conn, &volume_pubkey)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(volume.pubkey(), &volume_pubkey);
    assert_eq!(volume.account(), &account);
}

fn options_url(options: &Options) -> Result<Url> {
    Ok(Url::parse(&format!("http://{}", options.listen))?)
}

async fn wait_up(service: &Url) {
    let mut timer = tokio::time::interval(Duration::from_millis(20));
    let client = Client::new();
    loop {
        timer.tick().await;
        match health_check(service, &client).await {
            Ok(()) => break,
            Err(_) => {}
        }
    }
}

async fn wait_up_timeout(service: &Url) -> Result<()> {
    tokio::time::timeout(WAIT_UP_TIMEOUT, wait_up(service)).await?;
    Ok(())
}

fn options_default(listen: SocketAddr) -> Options {
    Options {
        database: "sqlite://:memory:".into(),
        ipfs: None,
        jwks: None,
        insecure_auth_stub: true,
        listen,
    }
}

async fn with_service<F>(test: impl FnOnce(Url) -> F) -> Result<()>
where
    F: Future<Output = Result<()>>,
{
    // initialize logger that is off by default but can be enabled using `RUST_LOG` env
    let _result = env_logger::builder()
        .parse_filters("off")
        .parse_default_env()
        .try_init();
    let port = thread_rng().gen_range(PORT_RANGE);
    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let options = options_default(listen);
    let url = options_url(&options)?;
    let service = tokio::spawn(async move {
        options.run().await.unwrap();
    });
    wait_up_timeout(&url).await?;
    test(url).await?;
    service.abort();
    Ok(())
}

#[tokio::test]
async fn can_launch_service() {
    with_service(|url| async move {
        health_check(&url, &Client::new()).await?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_volume_create() {
    with_service(|url| async move {
        let privkey = Privkey::generate();
        let client = Client::new();
        let token = Uuid::new_v4();
        volume_create(&url, &client, &token.to_string(), &privkey).await?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_volume_remove() {
    with_service(|url| async move {
        let privkey = Privkey::generate();
        let client = Client::new();
        let token = Uuid::new_v4();
        volume_create(&url, &client, &token.to_string(), &privkey).await?;
        volume_remove(&url, &client, &token.to_string(), &privkey).await?;
        let result = volume_remove(&url, &client, &token.to_string(), &privkey).await;
        assert!(result.is_err());
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_upload() {
    with_service(|url| async move {
        let volume = Privkey::generate();
        let client = Client::new();
        let token = Uuid::new_v4();
        let machine = Uuid::new_v4();
        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let manifest = Manifest {
            generation: 0,
            path: PathBuf::from_str("/tmp/path").unwrap(),
            creation: 0,
            machine,
            size: 10,
            size_total: 10,
            parent: None,
            data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
                .try_into()
                .unwrap(),
        };
        let manifest = manifest.sign(&volume);
        snapshot_upload(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            &manifest,
        )
        .await?;
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_fetch() {
    with_service(|url| async move {
        let volume = Privkey::generate();
        let client = Client::new();
        let token = Uuid::new_v4();
        let machine = Uuid::new_v4();
        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let manifest = Manifest {
            generation: 0,
            creation: 0,
            path: PathBuf::from_str("/tmp/path").unwrap(),
            machine,
            size: 10,
            size_total: 10,
            parent: None,
            data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
                .try_into()
                .unwrap(),
        };
        let manifest = manifest.sign(&volume);
        snapshot_upload(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            &manifest,
        )
        .await?;
        let signed_manifest =
            snapshot_fetch(&url, &client, &token.to_string(), &volume, &manifest.hash()).await?;
        assert_eq!(manifest, signed_manifest);
        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_fetch_missing() {
    with_service(|url| async move {
        let client = Client::new();
        let token = Uuid::new_v4();
        let volume = Privkey::generate();
        let snapshot = Hash::generate(&[]);
        let result = snapshot_fetch(&url, &client, &token.to_string(), &volume, &snapshot).await;
        assert!(matches!(
            result,
            Err(Error::Unsuccessful(StatusCode::NOT_FOUND))
        ));

        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let result = snapshot_fetch(&url, &client, &token.to_string(), &volume, &snapshot).await;
        assert!(matches!(
            result,
            Err(Error::Unsuccessful(StatusCode::NOT_FOUND))
        ));

        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_list_empty() {
    with_service(|url| async move {
        let client = Client::new();
        let token = Uuid::new_v4();
        let volume = Privkey::generate();

        // Listing snapshots on an nonexistant volume should return 404
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await;
        assert!(matches!(
            result,
            Err(Error::Unsuccessful(StatusCode::NOT_FOUND))
        ));

        // Listing snapshots on an empty volume should return an empty list.
        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await?;
        assert_eq!(result, vec![]);

        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_list_root() {
    with_service(|url| async move {
        let client = Client::new();
        let token = Uuid::new_v4();
        let machine = Uuid::new_v4();
        let volume = Privkey::generate();

        // Listing snapshots on an empty volume should return an empty list.
        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await?;
        assert_eq!(result, vec![]);

        // upload a single snapshot with no parent (root snapshot)
        let manifest = Manifest {
            generation: 0,
            creation: 0,
            path: PathBuf::from_str("/tmp/path").unwrap(),
            machine,
            size: 10,
            size_total: 10,
            parent: None,
            data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
                .try_into()
                .unwrap(),
        };
        let manifest = manifest.sign(&volume);
        snapshot_upload(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            &manifest,
        )
        .await?;

        // listing with no options should return this snapshot
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await?;
        assert_eq!(result, vec![manifest.hash()]);

        // listing with root set to true should return this snapshot
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            true,
        )
        .await?;
        assert_eq!(result, vec![manifest.hash()]);

        Ok(())
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn can_snapshot_list_child() {
    with_service(|url| async move {
        let client = Client::new();
        let token = Uuid::new_v4();
        let machine = Uuid::new_v4();
        let volume = Privkey::generate();

        // Listing snapshots on an empty volume should return an empty list.
        volume_create(&url, &client, &token.to_string(), &volume).await?;
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await?;
        assert_eq!(result, vec![]);

        // upload a single snapshot with no parent (root snapshot)
        let manifest = Manifest {
            generation: 0,
            creation: 0,
            path: PathBuf::from_str("/tmp/path").unwrap(),
            machine,
            size: crate::snapshot::MINIMUM_SNAPSHOT_SIZE,
            size_total: crate::snapshot::MINIMUM_SNAPSHOT_SIZE,
            parent: None,
            data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
                .try_into()
                .unwrap(),
        };
        let manifest = manifest.sign(&volume);
        let parent = manifest.hash();
        snapshot_upload(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            &manifest,
        )
        .await?;

        // upload a single snapshot with parent (child snapshot)
        let manifest = Manifest {
            generation: 1,
            creation: 0,
            path: PathBuf::from_str("/tmp/path").unwrap(),
            machine,
            size: crate::snapshot::MINIMUM_SNAPSHOT_SIZE,
            size_total: 2 * crate::snapshot::MINIMUM_SNAPSHOT_SIZE,
            parent: Some(Parent {
                hash: parent,
                volume: None,
            }),
            data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
                .try_into()
                .unwrap(),
        };
        let manifest = manifest.sign(&volume);
        let child = manifest.hash();
        snapshot_upload(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            &manifest,
        )
        .await?;

        // listing with no options should return all snapshots
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            false,
        )
        .await?;
        assert_eq!(result, vec![parent, child]);

        // listing with root set to true should return parent snapshot
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            None,
            true,
        )
        .await?;
        assert_eq!(result, vec![parent]);

        // listing with parent set to parent snapshot should return child snapshot
        let result = snapshot_list(
            &url,
            &client,
            &token.to_string(),
            &volume.pubkey(),
            Some(&parent),
            false,
        )
        .await?;
        assert_eq!(result, vec![child]);

        Ok(())
    })
    .await
    .unwrap();
}
