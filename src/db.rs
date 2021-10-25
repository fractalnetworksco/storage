use crate::keys::Pubkey;
use anyhow::Result;
use sqlx::sqlite::SqliteRow;
use sqlx::{query, Row, SqlitePool};

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
}
