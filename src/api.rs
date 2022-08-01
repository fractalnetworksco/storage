use crate::snapshot::{Snapshot, SnapshotError};
use crate::volume::{Volume, VolumeError};
use fractal_auth_client::UserContext;
use fractal_storage_client::{Hash, Pubkey, ManifestSigned, VolumeEdit, VolumeInfo};
use rocket::response::Redirect;
use rocket::response::status::BadRequest;
use rocket::{
    http::Status,
    request::Request,
    response::{self, Responder, Response},
    serde::json::Json,
    *,
};
use sqlx::AnyPool;
use std::io::Cursor;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Volume not found for user")]
    VolumeNotFound,
    #[error("Internal Error")]
    Internal,
    #[error("Error in snapshots: {0:}")]
    Snapshot(#[from] SnapshotError),
    #[error("Error in volume: {0:}")]
    Volume(#[from] VolumeError),
    #[error("Manifest Invalid")]
    ManifestInvalid,
    #[error("Snapshot not found")]
    SnapshotNotFound,
    #[error("Error talking to database: {0:}")]
    Database(#[from] sqlx::Error),
    #[error("Manifest for generation already exists but is different")]
    ManifestExists,
}

impl<'r> Responder<'r, 'static> for StorageError {
    fn respond_to(self, _: &'r Request<'_>) -> response::Result<'static> {
        ::log::error!("Responding with error: {self:?}");
        use StorageError::*;
        let status = match &self {
            VolumeNotFound => Status::NotFound,
            Internal => Status::InternalServerError,
            ManifestInvalid => Status::BadRequest,
            SnapshotNotFound => Status::NotFound,
            Snapshot(_) => Status::InternalServerError,
            Volume(_) => Status::InternalServerError,
            Database(_) => Status::InternalServerError,
            ManifestExists => Status::BadRequest,
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
    let mut conn = pool.acquire().await?;
    let account = Uuid::parse_str(&context.account().to_string()).unwrap();
    Volume::create(&mut conn, &volume, &account).await?;
    Ok(())
}

#[get("/volume/<volume>")]
async fn volume_get(
    _context: UserContext,
    pool: &State<AnyPool>,
    volume: Pubkey,
) -> Result<Json<VolumeInfo>, StorageError> {
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
        .ok_or(StorageError::VolumeNotFound)?;
    Ok(Json(VolumeInfo {
        account: volume.account().clone(),
        writer: volume.writer().cloned(),
    }))
}

#[delete("/volume/<volume>")]
async fn volume_delete(
    context: UserContext,
    pool: &State<AnyPool>,
    volume: Pubkey,
) -> Result<(), StorageError> {
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
        .ok_or(StorageError::VolumeNotFound)?;
    let account = Uuid::parse_str(&context.account().to_string()).unwrap();
    if volume.account() == &account {
        volume.delete(&mut conn).await?;
    }
    Ok(())
}

#[patch("/volume/<volume>", data = "<edit>")]
async fn volume_edit(
    _context: UserContext,
    pool: &State<AnyPool>,
    volume: Pubkey,
    edit: Json<VolumeEdit>,
) -> Result<(), StorageError> {
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
        .ok_or(StorageError::VolumeNotFound)?;
    volume.edit(&mut conn, &edit).await?;
    Ok(())
}

#[post("/volume/<volume>/snapshot", data = "<data>")]
async fn volume_snapshot_upload(
    _context: UserContext,
    data: Vec<u8>,
    pool: &State<AnyPool>,
    volume: Pubkey,
) -> Result<Redirect, StorageError> {
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
        .ok_or(StorageError::VolumeNotFound)?;
    let manifest_signed = ManifestSigned::parse(&data).map_err(|_| StorageError::ManifestInvalid)?;
    match Snapshot::fetch_by_generation(&mut conn, &volume.volume(), manifest_signed.manifest.generation).await? {
        // snapshot does not exist yet, all good.
        None => {},
        Some(snapshot) => {
            if *snapshot.manifest_signed() != manifest_signed {
                return Err(StorageError::ManifestExists);
            } else {
                info!("Existing manifest for volume {} generation {}", volume.pubkey(), manifest_signed.manifest.generation);
                return Ok(Redirect::to(snapshot.hash().to_hex()));
            }
        }
    };
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
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
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
    let snapshots = Snapshot::list(&mut conn, &volume.volume(), parent.as_ref(), root).await?;
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
    let mut conn = pool.acquire().await?;
    let volume = Volume::lookup(&mut conn, &volume)
        .await?
        .ok_or(StorageError::VolumeNotFound)?;
    let snapshot = Snapshot::fetch_by_hash(&mut conn, &volume.volume(), &snapshot)
        .await?
        .ok_or(StorageError::SnapshotNotFound)?;
    let manifest = snapshot.manifest_signed().data();
    Ok(manifest)
}

#[get("/health")]
async fn health_check() -> Result<(), String> {
    Ok(())
}

pub fn routes() -> Vec<Route> {
    routes![
        volume_create,
        volume_get,
        volume_edit,
        volume_delete,
        volume_snapshot_upload,
        volume_snapshot_get,
        volume_snapshot_list,
    ]
}

pub fn health() -> Vec<Route> {
    routes![health_check]
}
