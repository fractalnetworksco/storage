use crate::snapshot::{Snapshot, SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use crate::volume::Volume;
use crate::Options;
use fractal_auth_client::UserContext;
use rocket::{
    data::{ByteUnit, ToByteUnit},
    fs::TempFile,
    http::Status,
    request::{FromParam, Request},
    response::stream::ReaderStream,
    response::{self, Responder, Response},
    serde::json::Json,
    *,
};
use sqlx::{query, AnyPool};
use std::io::Cursor;
use storage_api::{Manifest, Pubkey, SnapshotInfo};
use thiserror::Error;
use tokio::fs::File;

#[derive(Error, Debug, Clone)]
pub enum StorageError {
    #[error("Volume not found for user")]
    VolumeNotFound,
    #[error("Internal Error")]
    Internal,
    #[error("Manifest Invalid")]
    ManifestInvalid,
}

impl<'r> Responder<'r, 'static> for StorageError {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        use StorageError::*;
        let status = match self {
            VolumeNotFound => Status::NotFound,
            Internal => Status::InternalServerError,
            ManifestInvalid => Status::BadRequest,
        };
        let message = self.to_string();
        let response = Response::build()
            .sized_body(message.len(), Cursor::new(message))
            .status(status)
            .ok();
        response
    }
}

pub fn snapshot_size_max() -> ByteUnit {
    1.terabytes()
}

#[post("/volume/<volume>")]
async fn volume_create(
    context: UserContext,
    pool: &State<AnyPool>,
    options: &State<Options>,
    volume: Pubkey,
) -> Result<(), StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    Volume::create(&mut conn, &volume, &context.account())
        .await
        .unwrap();
    Ok(())
}

#[post("/volume/<volume>/snapshot", data = "<data>")]
async fn snapshot_upload(
    data: Vec<u8>,
    pool: &State<AnyPool>,
    options: &State<Options>,
    volume: Pubkey,
) -> Result<Json<SnapshotInfo>, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    //.ok_or(|_| StorageError::VolumeNotFound)?;

    let (manifest, signature) = Manifest::split(&data).ok_or(StorageError::ManifestInvalid)?;

    Manifest::validate(manifest, signature, &volume).unwrap();

    let volume_data = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?;

    /*
    // parse header from snapshot data
    let header = data.peek(SNAPSHOT_HEADER_SIZE).await;
    let header = SnapshotHeader::from_bytes(header).unwrap();

    // TODO: check if snapshot exists
    if let Ok(Some(info)) =
        Snapshot::lookup(pool, volume.pubkey(), header.generation, header.parent).await
    {
        // TODO: make this return an error
        return Ok(Json(info.to_info()));
    }

    // open the entire data stream
    let data = data.open(snapshot_size_max());

    // write data stream to file
    let header_path = header.path(volume.pubkey());
    let path = options.storage.path().join(header_path.clone());
    let mut file = File::create(&path).await.unwrap();
    data.stream_to(tokio::io::BufWriter::new(&mut file)).await?;
    // TODO: generate hash to check signature

    let info = header.to_info(file.metadata().await?.len());
    volume
        .register(pool, &info, &header_path.to_str().unwrap())
        .await
        .unwrap();
    Ok(Json(info))
    */
    unimplemented!()
}

#[get("/volume/<volume>/latest?<parent>")]
async fn snapshot_latest(
    pool: &State<AnyPool>,
    parent: Option<u64>,
    volume: Pubkey,
) -> Json<Option<SnapshotInfo>> {
    let mut conn = pool.acquire().await.unwrap();
    let volume = Volume::lookup(&mut conn, &volume).await.unwrap().unwrap();
    let latest = Snapshot::latest(&mut conn, &volume, parent).await.unwrap();
    Json(latest.map(|inner| inner.to_info()))
}

#[get("/volume/<volume>/list?<parent>&<genmin>&<genmax>")]
async fn snapshot_list(
    pool: &State<AnyPool>,
    parent: Option<u64>,
    genmin: Option<u64>,
    genmax: Option<u64>,
    volume: Pubkey,
) -> Json<Vec<SnapshotInfo>> {
    let mut conn = pool.acquire().await.unwrap();
    let volume = Volume::lookup(&mut conn, &volume).await.unwrap().unwrap();
    let info = Snapshot::list(&mut conn, &volume, parent, genmin, genmax)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.to_info())
        .collect();
    Json(info)
}

#[get("/volume/<volume>/fetch?<generation>&<parent>")]
async fn snapshot_fetch(
    pool: &State<AnyPool>,
    options: &State<Options>,
    volume: Pubkey,
    generation: u64,
    parent: Option<u64>,
) -> ReaderStream![File] {
    let mut conn = pool.acquire().await.unwrap();
    let volume = Volume::lookup(&mut conn, &volume).await.unwrap().unwrap();
    let snapshot = volume
        .snapshot(&mut conn, generation, parent)
        .await
        .unwrap()
        .unwrap();
    let path = options.storage.path().join(snapshot.file());
    let file = File::open(path).await.unwrap();
    ReaderStream::one(file)
}

pub fn routes() -> Vec<Route> {
    routes![
        volume_create,
        snapshot_upload,
        snapshot_latest,
        snapshot_list,
        snapshot_fetch
    ]
}
