use crate::snapshot::{Snapshot, SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use crate::volume::Volume;
use crate::Options;
use fractal_auth_client::UserContext;
use rocket::http::Accept;
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
        .map_err(|_| StorageError::Internal)?;
    Ok(())
}

#[post("/volume/<volume>/snapshot", data = "<data>")]
async fn volume_snapshot_upload(
    data: Vec<u8>,
    pool: &State<AnyPool>,
    options: &State<Options>,
    volume: Pubkey,
) -> Result<Json<SnapshotInfo>, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let (manifest, signature) = Manifest::split(&data).ok_or(StorageError::ManifestInvalid)?;
    Manifest::validate(manifest, signature, &volume).unwrap();
    let manifest = Manifest::decode(manifest).map_err(|_| StorageError::ManifestInvalid)?;
    let volume_data = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?;
    unimplemented!()
}

#[get("/volume/<volume>/snapshots?<parent>&<genmin>&<genmax>")]
async fn volume_snapshot_list(
    pool: &State<AnyPool>,
    parent: Option<u64>,
    genmin: Option<u64>,
    genmax: Option<u64>,
    volume: Pubkey,
) -> Json<Vec<SnapshotInfo>> {
    /*
    let mut conn = pool.acquire().await.unwrap();
    let volume = Volume::lookup(&mut conn, &volume).await.unwrap().unwrap();
    let info = Snapshot::list(&mut conn, &volume, parent, genmin, genmax)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.to_info())
        .collect();
    Json(info)
    */
    unimplemented!()
}

#[get("/volume/<volume>/snapshot/<snapshot>?<format>")]
async fn volume_snapshot_get(
    pool: &State<AnyPool>,
    options: &State<Options>,
    volume: Pubkey,
    snapshot: String,
    format: Option<String>,
) -> Result<Vec<u8>, StorageError> {
    unimplemented!()
}

pub fn routes() -> Vec<Route> {
    routes![
        volume_create,
        volume_snapshot_upload,
        volume_snapshot_get,
        volume_snapshot_list,
    ]
}
