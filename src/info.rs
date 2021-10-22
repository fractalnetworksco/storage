use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{SqlitePool, Row, query};
use crate::keys::Pubkey;

pub const SNAPSHOT_HEADER_SIZE: usize = 4 * 8;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotInfo {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: Option<u64>,
    pub size: u64,
}

impl SnapshotInfo {
    pub fn from_row(row: &SqliteRow) -> Result<Self> {
        let generation: i64 = row.try_get("snapshot_generation")?;
        let parent: Option<i64> = row.try_get("snapshot_parent")?;
        let size: i64 = row.try_get("snapshot_size")?;
        Ok(SnapshotInfo {
            generation: generation.try_into()?,
            parent: parent.map(|parent| parent as u64),
            creation: None,
            size: size.try_into()?,
        })
    }

    pub async fn latest(pool: &SqlitePool, volume: &Pubkey, parent: Option<u64>) -> Result<Self> {
        let row = query(
            "SELECT * FROM storage_snapshot
                JOIN storage_volume
                    ON storage_volume.volume_id = storage_snapshot.volume_id
                WHERE volume_pubkey = ?
                    AND snapshot_parent = ?",
        )
        .bind(volume.as_slice())
        .bind(parent.map(|parent| parent as i64))
        .fetch_one(pool)
        .await
        .unwrap();
        Ok(Self::from_row(&row).unwrap())
    }

    pub fn from_header(data: &[u8]) -> Result<Self> {
        unimplemented!()
    }
}
