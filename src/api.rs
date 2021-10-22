use crate::info::{SnapshotInfo, SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use crate::keys::Pubkey;
use crate::Options;
use crate::db::Volume;
use rocket::data::{ByteUnit, ToByteUnit};
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::*;
use sqlx::{query, SqlitePool};
use tokio::fs::File;

pub fn snapshot_size_max() -> ByteUnit {
    1.terabytes()
}

#[post("/snapshot/<volume>/create")]
async fn volume_create(
    pool: &State<SqlitePool>,
    options: &State<Options>,
    volume: Pubkey,
) -> () {
    Volume::create(pool, &volume).await.unwrap();
    ()
}

#[post("/snapshot/<volume>/upload", data = "<data>")]
async fn snapshot_upload(
    mut data: Data<'_>,
    pool: &State<SqlitePool>,
    options: &State<Options>,
    volume: Pubkey,
) -> std::io::Result<()> {
    // parse header from snapshot data
    let header = data.peek(SNAPSHOT_HEADER_SIZE).await;
    let header = SnapshotHeader::from_bytes(header).unwrap();

    // TODO: check if snapshot exists
    if let Ok(Some(info)) =
        SnapshotInfo::lookup(pool, &volume, header.generation, header.parent).await
    {
        return Ok(());
    }

    // open the entire data stream
    let data = data.open(snapshot_size_max());

    // write data stream to file
    let path = options.storage.join(header.path(&volume));
    tokio::fs::create_dir(path.parent().unwrap()).await?;
    let mut file = File::create(&path).await.unwrap();
    data.stream_to(tokio::io::BufWriter::new(&mut file)).await?;
    // TODO: generate hash to check signature

    let header = header.to_info(file.metadata().await?.len());
    header.register(pool, &volume).await.unwrap();
    Ok(())
}

#[get("/snapshot/<volume>/latest?<parent>")]
async fn snapshot_latest(
    pool: &State<SqlitePool>,
    parent: Option<u64>,
    volume: Pubkey,
) -> Json<SnapshotInfo> {
    let info = SnapshotInfo::latest(pool, &volume, parent).await.unwrap();
    Json(info)
}

#[get("/snapshot/<volume>/fetch?<generation>&<parent>")]
async fn snapshot_fetch(volume: Pubkey, generation: u64, parent: Option<u64>) -> String {
    unimplemented!()
}

pub fn routes() -> Vec<Route> {
    routes![volume_create, snapshot_upload, snapshot_latest, snapshot_fetch]
}
