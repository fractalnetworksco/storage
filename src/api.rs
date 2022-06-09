use crate::snapshot::{Snapshot, SnapshotError, SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use crate::volume::Volume;
use crate::Options;
use fractal_auth_client::UserContext;
use rocket::http::Accept;
use rocket::response::Redirect;
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
use storage_api::{Hash, Manifest, Pubkey, SnapshotInfo};
use thiserror::Error;
use tokio::fs::File;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Volume not found for user")]
    VolumeNotFound,
    #[error("Internal Error")]
    Internal,
    #[error("Internal Error: {0:}")]
    Snapshot(#[from] SnapshotError),
    #[error("Manifest Invalid")]
    ManifestInvalid,
    #[error("Format invalid")]
    FormatInvalid,
    #[error("Snapshot not found")]
    SnapshotNotFound,
}

impl<'r> Responder<'r, 'static> for StorageError {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        use StorageError::*;
        let status = match self {
            VolumeNotFound => Status::NotFound,
            Internal => Status::InternalServerError,
            ManifestInvalid => Status::BadRequest,
            SnapshotNotFound => Status::NotFound,
            FormatInvalid => Status::BadRequest,
            Snapshot(_) => Status::InternalServerError,
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
    volume: Pubkey,
) -> Result<(), StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let account = Uuid::parse_str(&context.account().to_string()).unwrap();
    Volume::create(&mut conn, &volume, &account)
        .await
        .map_err(|_| StorageError::Internal)?;
    Ok(())
}

#[delete("/volume/<volume>")]
async fn volume_delete(
    context: UserContext,
    pool: &State<AnyPool>,
    volume: Pubkey,
) -> Result<(), StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?
        .ok_or(StorageError::VolumeNotFound)?;
    let account = Uuid::parse_str(&context.account().to_string()).unwrap();
    if volume.account() == &account {
        volume
            .delete(&mut conn)
            .await
            .map_err(|_| StorageError::Internal)?;
    }
    Ok(())
}

#[post("/volume/<volume>/snapshot", data = "<data>")]
async fn volume_snapshot_upload(
    data: Vec<u8>,
    pool: &State<AnyPool>,
    volume: Pubkey,
) -> Result<Redirect, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?
        .ok_or(StorageError::VolumeNotFound)?;
    let snapshot = Snapshot::create_from_manifest(&mut conn, &volume, &data).await?;
    let snapshot = snapshot.fetch(&mut conn).await?;
    Ok(Redirect::to(snapshot.hash().to_hex()))
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

#[get("/volume/<volume>/snapshot/<snapshot>")]
async fn volume_snapshot_get(
    pool: &State<AnyPool>,
    volume: Pubkey,
    snapshot: Hash,
) -> Result<Vec<u8>, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let volume = Volume::lookup(&mut conn, &volume).await.unwrap().unwrap();
    let snapshot = Snapshot::fetch_by_hash(&mut conn, &volume.volume(), &snapshot)
        .await?
        .ok_or(StorageError::SnapshotNotFound)?;
    let mut manifest = snapshot.manifest().to_vec();
    // FIXME: append
    //manifest.append(snapshot.signature());
    Ok(manifest)
}

#[get("/health")]
async fn health_check() -> Result<(), String> {
    Ok(())
}

pub fn routes() -> Vec<Route> {
    routes![
        volume_create,
        volume_delete,
        volume_snapshot_upload,
        volume_snapshot_get,
        volume_snapshot_list,
    ]
}

pub fn health() -> Vec<Route> {
    routes![health_check]
}
