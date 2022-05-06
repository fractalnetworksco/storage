use crate::volume::Volume;
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use sqlx::any::AnyRow;
use sqlx::{query, AnyConnection, AnyPool, Row, SqlitePool};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use storage_api::{Manifest, Pubkey, SnapshotInfo};
pub use storage_api::{SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum SnapshotError {
    #[error("Manifest Invalid")]
    ManifestInvalid,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    id: i64,
    volume: i64,
    parent: Option<i64>,
    manifest: Vec<u8>,
    signature: Vec<u8>,
    hash: Vec<u8>,
}

impl Snapshot {
    pub fn from_row(row: &AnyRow) -> Result<Self> {
        let id: i64 = row.try_get("snapshot_id")?;
        let volume: i64 = row.try_get("snapshot_volume")?;
        let hash: Vec<u8> = row.try_get("snapshot_hash")?;
        let parent: Option<i64> = row.try_get("snapshot_parent")?;
        let manifest: Vec<u8> = row.try_get("snapshot_manifest")?;
        let signature: Vec<u8> = row.try_get("snapshot_signature")?;
        Ok(Snapshot {
            id,
            volume,
            parent,
            manifest,
            signature,
            hash,
        })
    }

    pub fn from_manifest(conn: &mut AnyConnection, volume: &Volume, manifest: &[u8]) -> Result<()> {
        let (manifest, signature) =
            Manifest::split(&manifest).ok_or(SnapshotError::ManifestInvalid)?;
        Manifest::validate(manifest, signature, volume.pubkey()).unwrap();
        let manifest = Manifest::decode(manifest).map_err(|_| SnapshotError::ManifestInvalid)?;

        unimplemented!()
    }

    pub async fn lookup(conn: &mut AnyConnection, volume: &Pubkey) -> Result<Option<Self>> {
        /*
        let row = query(
            "SELECT * FROM storage_snapshot
                JOIN storage_volume
                    ON storage_volume.volume_id = storage_snapshot.volume_id
                WHERE volume_pubkey = ?
                    AND snapshot_generation = ?
                    AND snapshot_parent IS ?",
        )
        .bind(volume.as_slice())
        .bind(generation as i64)
        .bind(parent.map(|parent| parent as i64))
        .fetch_optional(conn)
        .await
        .unwrap();
        match row {
            Some(row) => Ok(Some(Self::from_row(&row)?)),
            None => Ok(None),
        }
        */
        unimplemented!()
    }

    pub async fn list(
        conn: &mut AnyConnection,
        volume: &Volume,
        parent: Option<u64>,
    ) -> Result<Vec<Self>> {
        /*
        let rows = query(
            "SELECT * FROM storage_snapshot
                WHERE volume_id = $1
                AND ($2 IS NULL OR snapshot_parent = $2)
                AND ($3 IS NULL OR snapshot_generation >= $3)
                AND ($4 IS NULL OR snapshot_generation <= $4)",
        )
        .bind(volume.id() as i64)
        .bind(parent.map(|parent| parent as i64))
        .bind(genmin.map(|parent| parent as i64))
        .bind(genmax.map(|parent| parent as i64))
        .fetch_all(conn)
        .await
        .unwrap();
        let mut snapshots = vec![];
        for row in &rows {
            snapshots.push(Self::from_row(row)?);
        }
        Ok(snapshots)
        */
        unimplemented!()
    }
}
