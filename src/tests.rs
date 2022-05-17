use crate::volume::Volume;
use sqlx::AnyPool;
use storage_api::Privkey;
use uuid::Uuid;

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
