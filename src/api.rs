use crate::snapshot::{Snapshot, SnapshotError};
use crate::volume::Volume;
use fractal_auth_client::UserContext;
use rocket::response::Redirect;
use rocket::{
    http::Status,
    request::Request,
    response::{self, Responder, Response},
    serde::json::Json,
    *,
};
use sqlx::AnyPool;
use std::io::Cursor;
use storage_api::{Hash, Pubkey};
use thiserror::Error;
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
        ::log::error!("Responding with error: {self:?}");
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
    _context: UserContext,
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

#[get("/volume/<volume>/snapshots?<parent>&<root>")]
async fn volume_snapshot_list(
    _context: UserContext,
    pool: &State<AnyPool>,
    volume: Pubkey,
    parent: Option<Hash>,
    root: bool,
) -> Result<Json<Vec<Hash>>, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?
        .ok_or(StorageError::VolumeNotFound)?;
    let parent = match parent {
        Some(hash) => Some(
            Snapshot::fetch_by_hash(&mut conn, &volume.volume(), &hash)
                .await?
                .ok_or_else(|| StorageError::SnapshotNotFound)?
                .snapshot(),
        ),
        None => None,
    };
    let snapshots = Snapshot::list(&mut conn, &volume.volume(), parent.as_ref(), root)
        .await
        .map_err(|_| StorageError::Internal)?;
    Ok(Json(
        snapshots.iter().map(|snapshot| snapshot.hash()).collect(),
    ))
}

#[get("/volume/<volume>/<snapshot>")]
async fn volume_snapshot_get(
    pool: &State<AnyPool>,
    volume: Pubkey,
    snapshot: Hash,
) -> Result<Vec<u8>, StorageError> {
    let mut conn = pool.acquire().await.map_err(|_| StorageError::Internal)?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await
        .map_err(|_| StorageError::Internal)?
        .ok_or(StorageError::VolumeNotFound)?;
    let snapshot = Snapshot::fetch_by_hash(&mut conn, &volume.volume(), &snapshot)
        .await?
        .ok_or(StorageError::SnapshotNotFound)?;
    let mut manifest = snapshot.manifest().to_vec();
    manifest.extend_from_slice(snapshot.signature());
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
