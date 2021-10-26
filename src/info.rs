use crate::db::Volume;
use crate::keys::Pubkey;
use anyhow::Result;
use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{query, Row, SqlitePool};
use std::ffi::OsString;
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub const SNAPSHOT_HEADER_SIZE: usize = 4 * 8;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotHeader {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotInfo {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    generation: u64,
    parent: Option<u64>,
    size: u64,
    time: u64,
    file: PathBuf,
}

impl SnapshotHeader {
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        let mut reader = Cursor::new(data);
        Ok(SnapshotHeader {
            generation: reader.read_u64::<BigEndian>()?,
            parent: match reader.read_u64::<BigEndian>()? {
                0 => None,
                value => Some(value),
            },
            creation: reader.read_u64::<BigEndian>()?,
        })
    }

    pub fn to_info(&self, size: u64) -> SnapshotInfo {
        SnapshotInfo {
            generation: self.generation,
            parent: self.parent,
            size: size,
            creation: self.creation,
        }
    }

    pub fn path(&self, volume: &Pubkey) -> PathBuf {
        let mut path = PathBuf::new();
        path.push(volume.to_hex());
        if let Some(parent) = self.parent {
            path.push(format!("{}-{}.snap", self.generation, parent));
        } else {
            path.push(format!("{}.snap", self.generation));
        }
        path
    }
}

impl Snapshot {
    pub fn from_row(row: &SqliteRow) -> Result<Self> {
        let generation: i64 = row.try_get("snapshot_generation")?;
        let parent: Option<i64> = row.try_get("snapshot_parent")?;
        let size: i64 = row.try_get("snapshot_size")?;
        let creation: i64 = row.try_get("snapshot_time")?;
        let file: String = row.try_get("snapshot_file")?;
        Ok(Snapshot {
            generation: generation.try_into()?,
            parent: parent.map(|parent| parent as u64),
            time: creation.try_into()?,
            size: size.try_into()?,
            file: PathBuf::from(&file),
        })
    }

    pub async fn latest(
        pool: &SqlitePool,
        volume: &Volume,
        parent: Option<u64>,
    ) -> Result<Option<Self>> {
        let row = query(
            "SELECT * FROM storage_snapshot
                WHERE volume_id = ?
                    AND snapshot_parent IS ?",
        )
        .bind(volume.id() as i64)
        .bind(parent.map(|parent| parent as i64))
        .fetch_optional(pool)
        .await?;
        match row {
            Some(row) => Ok(Some(Self::from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn lookup(
        pool: &SqlitePool,
        volume: &Pubkey,
        generation: u64,
        parent: Option<u64>,
    ) -> Result<Option<Self>> {
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
        .fetch_optional(pool)
        .await
        .unwrap();
        match row {
            Some(row) => Ok(Some(Self::from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn list(
        pool: &SqlitePool,
        volume: &Volume,
        parent: Option<u64>,
        genmin: Option<u64>,
        genmax: Option<u64>,
    ) -> Result<Vec<Self>> {
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
        .fetch_all(pool)
        .await
        .unwrap();
        let mut snapshots = vec![];
        for row in &rows {
            snapshots.push(Self::from_row(row)?);
        }
        Ok(snapshots)
    }

    pub async fn exists(&self, pool: &SqlitePool, volume: &Pubkey) -> Result<bool> {
        Ok(false)
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn to_info(&self) -> SnapshotInfo {
        SnapshotInfo {
            generation: self.generation,
            parent: self.parent,
            creation: self.time,
            size: self.size,
        }
    }
}
