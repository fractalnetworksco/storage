use crate::volume::Volume;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use byteorder::{BigEndian, ReadBytesExt};
use rocket::serde::uuid::Uuid;
use serde::{Deserialize, Serialize};
use sqlx::any::AnyRow;
use sqlx::{query, AnyConnection, AnyPool, Row, SqlitePool};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use storage_api::{Manifest, Privkey, Pubkey, SnapshotInfo};
pub use storage_api::{SnapshotHeader, SNAPSHOT_HEADER_SIZE};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SnapshotError {
    #[error("Manifest Invalid")]
    ManifestInvalid,
    #[error("Database error: {0:}")]
    Database(#[from] sqlx::Error),
    #[error("Missing rowid")]
    MissingRowid,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Snapshot(i64);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotData {
    id: i64,
    volume: i64,
    parent: Option<i64>,
    manifest: Vec<u8>,
    signature: Vec<u8>,
    hash: Vec<u8>,
}

#[async_trait]
pub trait SnapshotExt {
    fn snapshot(&self) -> Snapshot;
}

#[async_trait]
impl SnapshotExt for Snapshot {
    fn snapshot(&self) -> Snapshot {
        *self
    }
}

#[async_trait]
impl SnapshotExt for SnapshotData {
    fn snapshot(&self) -> Snapshot {
        self.snapshot()
    }
}

impl SnapshotData {
    pub fn from_row(row: &AnyRow) -> Result<Self, SnapshotError> {
        let id: i64 = row.try_get("snapshot_id")?;
        let volume: i64 = row.try_get("volume_id")?;
        let hash: Vec<u8> = row.try_get("snapshot_hash")?;
        let parent: Option<i64> = row.try_get("snapshot_parent")?;
        let manifest: Vec<u8> = row.try_get("snapshot_manifest")?;
        let signature: Vec<u8> = row.try_get("snapshot_signature")?;
        Ok(SnapshotData {
            id,
            volume,
            parent,
            manifest,
            signature,
            hash,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.id)
    }

    pub fn manifest(&self) -> &[u8] {
        &self.manifest
    }

    pub fn signature(&self) -> &[u8] {
        &self.signature
    }

    pub fn hash(&self) -> &[u8] {
        &self.hash
    }
}

impl Snapshot {
    pub fn id(&self) -> i64 {
        self.0
    }

    pub async fn create(
        conn: &mut AnyConnection,
        volume: &Volume,
        manifest: &[u8],
        signature: &[u8],
        hash: &[u8],
        parent: Option<&Snapshot>,
    ) -> Result<Snapshot, SnapshotError> {
        let result = query(
            "INSERT INTO storage_snapshot(
            volume_id,
            snapshot_manifest,
            snapshot_signature,
            snapshot_hash,
            snapshot_parent)
            VALUES (?, ?, ?, ?, ?)",
        )
        .bind(volume.id())
        .bind(manifest)
        .bind(signature)
        .bind(hash)
        .bind(parent.map(|p| p.id()))
        .execute(conn)
        .await?;
        Ok(Snapshot(
            result.last_insert_id().ok_or(SnapshotError::MissingRowid)?,
        ))
    }

    pub async fn from_manifest(
        conn: &mut AnyConnection,
        volume: &Volume,
        manifest: &[u8],
    ) -> Result<Snapshot, SnapshotError> {
        let (manifest, signature) =
            Manifest::split(&manifest).ok_or(SnapshotError::ManifestInvalid)?;
        Manifest::validate(manifest, signature, volume.pubkey()).unwrap();
        let parsed = Manifest::decode(manifest).map_err(|_| SnapshotError::ManifestInvalid)?;

        let hash = Manifest::hash(manifest);

        // FIXME: parent
        let snapshot = Snapshot::create(conn, volume, manifest, signature, &hash, None).await?;

        Ok(snapshot)
    }

    pub async fn fetch(&self, conn: &mut AnyConnection) -> Result<SnapshotData, SnapshotError> {
        let row = query("SELECT * FROM storage_snapshot WHERE snapshot_id = ?")
            .bind(self.0)
            .fetch_one(conn)
            .await?;
        Ok(SnapshotData::from_row(&row)?)
    }

    pub async fn fetch_by_hash(
        conn: &mut AnyConnection,
        hash: &[u8],
    ) -> Result<Option<Snapshot>, SnapshotError> {
        let row = query("SELECT snapshot_id FROM storage_snapshow WHERE snapshot_hash = ?")
            .bind(hash)
            .fetch_optional(conn)
            .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(Snapshot(row.try_get("snapshot_id")?))),
        }
    }

    pub async fn lookup(conn: &mut AnyConnection, volume: &Pubkey) -> Result<Option<Snapshot>> {
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

#[tokio::test]
async fn test_snapshot_create() {
    // create and connect database
    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();

    // create volume
    let account = Uuid::new_v4();
    let privkey = Privkey::generate();
    let pubkey = privkey.pubkey();
    Volume::create(&mut conn, &pubkey, &account).await.unwrap();
    let volume = Volume::lookup(&mut conn, &pubkey).await.unwrap().unwrap();

    let manifest = vec![66; 60];
    let signature = vec![14; 24];
    let hash = vec![12; 16];
    let snapshot = Snapshot::create(&mut conn, &volume, &manifest, &signature, &hash, None)
        .await
        .unwrap();

    let snapshot_data = snapshot.fetch(&mut conn).await.unwrap();
    assert_eq!(snapshot_data.snapshot(), snapshot);

    assert_eq!(snapshot_data.manifest(), manifest);
    assert_eq!(snapshot_data.signature(), signature);
    assert_eq!(snapshot_data.hash(), hash);
}
