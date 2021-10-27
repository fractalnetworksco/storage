use crate::info::Snapshot;
use crate::keys::Pubkey;
use anyhow::Result;
use sqlx::sqlite::SqliteRow;
use sqlx::{query, Row, SqlitePool};
use std::path::Path;
use storage_api::SnapshotInfo;

#[derive(Clone, Debug)]
pub struct Volume {
    id: u64,
    pubkey: Pubkey,
}

impl Volume {
    pub async fn create(pool: &SqlitePool, pubkey: &Pubkey) -> Result<()> {
        let result = query(
            "INSERT INTO storage_volume(volume_pubkey)
            VALUES (?)",
        )
        .bind(pubkey.as_slice())
        .execute(pool)
        .await?;
        Ok(())
    }

    pub async fn lookup(pool: &SqlitePool, pubkey: &Pubkey) -> Result<Option<Self>> {
        let result = query(
            "SELECT * FROM storage_volume
                WHERE volume_pubkey = ?",
        )
        .bind(pubkey.as_slice())
        .fetch_optional(pool)
        .await?;
        if let Some(result) = result {
            Ok(Some(Volume::from_row(&result)?))
        } else {
            Ok(None)
        }
    }

    pub fn from_row(row: &SqliteRow) -> Result<Self> {
        let id: i64 = row.try_get("volume_id")?;
        let key: &[u8] = row.try_get("volume_pubkey")?;
        Ok(Volume {
            id: id.try_into()?,
            pubkey: Pubkey(key.try_into()?),
        })
    }

    pub fn pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub async fn register(
        &self,
        pool: &SqlitePool,
        snapshot: &SnapshotInfo,
        file: &str,
    ) -> Result<()> {
        query(
            "INSERT INTO storage_snapshot(volume_id, snapshot_generation, snapshot_parent, snapshot_time, snapshot_size, snapshot_file)
                VALUES (?, ?, ?, ?, ?, ?)")
            .bind(self.id as i64)
            .bind(snapshot.generation as i64)
            .bind(snapshot.parent.map(|i| i as i64))
            .bind(snapshot.creation as i64)
            .bind(snapshot.size as i64)
            .bind(file)
            .execute(pool)
            .await?;
        Ok(())
    }

    pub async fn snapshot(
        &self,
        pool: &SqlitePool,
        generation: u64,
        parent: Option<u64>,
    ) -> Result<Option<Snapshot>> {
        let row = query(
            "SELECT * FROM storage_snapshot
                WHERE volume_id = ?
                    AND snapshot_generation = ?
                    AND snapshot_parent IS ?",
        )
        .bind(self.id as i64)
        .bind(generation as i64)
        .bind(parent.map(|parent| parent as i64))
        .fetch_optional(pool)
        .await
        .unwrap();
        match row {
            Some(row) => Ok(Some(Snapshot::from_row(&row)?)),
            None => Ok(None),
        }
    }
}
