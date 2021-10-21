use crate::info::SnapshotInfo;
use crate::keys::Pubkey;
use rocket::data::ToByteUnit;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::*;
use sqlx::{query, SqlitePool};

#[post("/snapshot/<volume>/upload", data = "<data>")]
async fn upload(mut data: TempFile<'_>, volume: Pubkey) -> std::io::Result<()> {
    //file.persist_to(permanent_location).await
    Ok(())
}

#[get("/snapshot/<volume>/latest?<parent>")]
async fn latest(
    pool: &State<SqlitePool>,
    parent: Option<u64>,
    volume: Pubkey,
) -> Json<SnapshotInfo> {
    let row = query(
        "SELECT * FROM storage_snapshot
            JOIN storage_volume
                ON storage_volume.volume_id = storage_snapshot.volume_id
            WHERE volume_pubkey = ?
                AND snapshot_parent = ?",
    )
    .bind(volume.as_slice())
    .bind(parent.map(|parent| parent as i64))
    .fetch_one(pool.inner())
    .await
    .unwrap();
    Json(SnapshotInfo::from_row(&row).unwrap())
}

#[get("/snapshot/<volume>/fetch?<generation>&<parent>")]
async fn fetch(volume: Pubkey, generation: u64, parent: Option<u64>) -> String {
    unimplemented!()
}

pub fn routes() -> Vec<Route> {
    routes![upload, latest, fetch]
}
