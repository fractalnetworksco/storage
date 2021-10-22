use sqlx::{query, SqlitePool};
use crate::keys::Pubkey;
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct Volume {
    id: u64,
    pubkey: Pubkey,
}

impl Volume {
    pub async fn create(pool: &SqlitePool, pubkey: &Pubkey) -> Result<()> {
        let result = query(
            "INSERT INTO storage_volume(volume_pubkey)
            VALUES (?)")
            .bind(pubkey.as_slice())
            .execute(pool)
            .await?;
        Ok(())
    }
}
