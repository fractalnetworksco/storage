use anyhow::Result;
use crate::volume::Volume;
use crate::Options;
use sqlx::AnyPool;
use storage_api::*;
use uuid::Uuid;
use std::time::Duration;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};
use std::future::Future;
use url::Url;
use reqwest::Client;
use rand::{thread_rng, Rng};
use std::ops::Range;

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
            Err(_) => {},
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
        insecure_auth: true,
        listen,
    }
}

async fn with_service<F>(test: impl FnOnce(Url) -> F) -> Result<()> where F: Future<Output = Result<()>> {
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
    }).await.unwrap();
}

#[tokio::test]
async fn can_volume_create() {
    with_service(|url| async move {
        Ok(())
    }).await.unwrap();
}
