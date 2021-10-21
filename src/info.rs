use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::Row;

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
}
