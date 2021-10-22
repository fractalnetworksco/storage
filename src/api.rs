use crate::info::{SnapshotInfo, SNAPSHOT_HEADER_SIZE};
use crate::keys::Pubkey;
use rocket::data::{ByteUnit, ToByteUnit};
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::*;
use sqlx::{query, SqlitePool};
use tokio::fs::File;

pub fn snapshot_size_max() -> ByteUnit {
    1.terabytes()
}

#[post("/snapshot/<volume>/upload", data = "<data>")]
async fn upload(
    mut data: Data<'_>,
    pool: &State<SqlitePool>,
    volume: Pubkey,
) -> std::io::Result<()> {
    // parse header from snapshot data
    let header = data.peek(SNAPSHOT_HEADER_SIZE).await;
    let header = SnapshotInfo::from_header(header).unwrap();

    // TODO: check if snapshot exists

    // open the entire data stream
    let data = data.open(snapshot_size_max());

    // write data stream to file
    let path = header.path(&volume);
    let mut file = File::create(&path).await.unwrap();
    data.stream_to(tokio::io::BufWriter::new(&mut file)).await?;
    // TODO: generate hash to check signature

    header.register(pool, &volume).await.unwrap();
    Ok(())
}

#[get("/snapshot/<volume>/latest?<parent>")]
async fn latest(
    pool: &State<SqlitePool>,
    parent: Option<u64>,
    volume: Pubkey,
) -> Json<SnapshotInfo> {
    let info = SnapshotInfo::latest(pool, &volume, parent).await.unwrap();
    Json(info)
}

#[get("/snapshot/<volume>/fetch?<generation>&<parent>")]
async fn fetch(volume: Pubkey, generation: u64, parent: Option<u64>) -> String {
    unimplemented!()
}

pub fn routes() -> Vec<Route> {
    routes![upload, latest, fetch]
}
