use crate::info::{SNAPSHOT_HEADER_SIZE, SnapshotInfo};
use crate::keys::Pubkey;
use rocket::data::ToByteUnit;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::*;
use sqlx::{query, SqlitePool};

#[post("/snapshot/<volume>/upload", data = "<data>")]
async fn upload(mut data: Data<'_>, volume: Pubkey) -> std::io::Result<()> {
    let header = data.peek(SNAPSHOT_HEADER_SIZE).await;
    let header = SnapshotInfo::from_header(header).unwrap();
    //file.persist_to(permanent_location).await
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
