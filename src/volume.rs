use crate::snapshot::{SnapshotData, SnapshotError};
use fractal_storage_client::{Pubkey, SnapshotInfo, VolumeEdit};
use optional_field::Field;
use sqlx::any::AnyRow;
use sqlx::{query, AnyConnection, Row};
use std::str::FromStr;
use uuid::Uuid;


/// Represents the primary key of a row in the storage_volume table
#[derive(Clone, Debug)]
pub struct Volume(i64);


/// Represents a row in the storage_volume table
#[derive(Clone, Debug)]
pub struct VolumeData {
    /// Primary key of the volume in the storage_volume table.
    id: i64,
    /// ED25519 public key of the volume.
    pubkey: Pubkey,
    /// Account to which the volume belongs.
    account: Uuid,
    /// Device UUID of the current writer of the volume.
    writer: Option<Uuid>,
    /// Prevent any changes to the volume in the database.
    locked: bool,
}

#[derive(thiserror::Error, Debug)]
pub enum VolumeError {
    #[error("Error talking to database: {0:}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Error inserting data: missing rowid")]
    MissingRowid,
    #[error("Error parsing UUID: {0:}")]
    ParseUuid(#[from] uuid::Error),
    #[error("Error parsing key: {0:}")]
    ParseKey(#[from] fractal_storage_client::keys::ParseError),
}

impl VolumeData {
    pub fn from_row(row: &AnyRow) -> Result<Self, VolumeError> {
        let id: i64 = row.try_get("volume_id")?;
        let key: &[u8] = row.try_get("volume_pubkey")?;
        let account: &str = row.try_get("account_id")?;
        let account = Uuid::from_str(account)?;
        let writer: Option<&str> = row.try_get("volume_writer")?;
        let writer = writer.map(|w| Uuid::from_str(w)).transpose()?;
        Ok(VolumeData {
            id,
            pubkey: Pubkey::try_from(key)?,
            account,
            writer,
            locked: row.try_get("volume_locked")?,
        })
    }

    pub async fn delete(&self, conn: &mut AnyConnection) -> Result<(), VolumeError> {
        query("DELETE FROM storage_volume WHERE volume_id = ?")
            .bind(self.id)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub fn pubkey(&self) -> &Pubkey {
        &self.pubkey
    }

    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn volume(&self) -> Volume {
        Volume(self.id)
    }

    pub fn account(&self) -> &Uuid {
        &self.account
    }

    pub fn writer(&self) -> Option<&Uuid> {
        self.writer.as_ref()
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub async fn register(
        &self,
        conn: &mut AnyConnection,
        snapshot: &SnapshotInfo,
        file: &str,
    ) -> Result<(), VolumeError> {
        query(
            "INSERT INTO storage_snapshot(volume_id, snapshot_generation, snapshot_parent, snapshot_time, snapshot_size, snapshot_file)
                VALUES (?, ?, ?, ?, ?, ?)")
            .bind(self.id as i64)
            .bind(snapshot.generation as i64)
            .bind(snapshot.parent.map(|i| i as i64))
            .bind(snapshot.creation as i64)
            .bind(snapshot.size as i64)
            .bind(file)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn snapshot(
        &self,
        conn: &mut AnyConnection,
        generation: u64,
        parent: Option<u64>,
    ) -> Result<Option<SnapshotData>, SnapshotError> {
        let row = query(
            "SELECT * FROM storage_snapshot
                WHERE volume_id = ?
                    AND snapshot_generation = ?
                    AND snapshot_parent IS ?",
        )
        .bind(self.id as i64)
        .bind(generation as i64)
        .bind(parent.map(|parent| parent as i64))
        .fetch_optional(conn)
        .await
        .unwrap();
        match row {
            Some(row) => Ok(Some(SnapshotData::from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn edit(
        &self,
        conn: &mut AnyConnection,
        edit: &VolumeEdit,
    ) -> Result<(), VolumeError> {
        if let Field::Present(value) = &edit.writer {
            if &self.writer != value {
                self.volume().writer_set(conn, value.as_ref()).await?;
            }
        }
        if let Some(value) = &edit.account {
            if &self.account != value {
                self.volume().account_set(conn, &value).await?;
            }
        }
        if let Some(value) = &edit.lock {
            if &self.locked != value {
                self.volume().locked_set(conn, *value).await?;
            }
        }
        Ok(())
    }
}

impl Volume {
    pub async fn create(
        conn: &mut AnyConnection,
        pubkey: &Pubkey,
        account: &Uuid,
    ) -> Result<Self, VolumeError> {
        let result = query(
            "INSERT INTO storage_volume(volume_pubkey, account_id)
            VALUES (?, ?)",
        )
        .bind(pubkey.as_slice())
        .bind(account.to_string())
        .execute(conn)
        .await?;
        Ok(Volume(
            result.last_insert_id().ok_or(VolumeError::MissingRowid)?,
        ))
    }

    pub async fn lookup(
        conn: &mut AnyConnection,
        pubkey: &Pubkey,
    ) -> Result<Option<VolumeData>, VolumeError> {
        let result = query(
            "SELECT * FROM storage_volume
                WHERE volume_pubkey = ?",
        )
        .bind(pubkey.as_slice())
        .fetch_optional(conn)
        .await?;
        if let Some(result) = result {
            Ok(Some(VolumeData::from_row(&result)?))
        } else {
            Ok(None)
        }
    }

    pub fn from_row(row: &AnyRow) -> Result<Self, VolumeError> {
        let id: i64 = row.try_get("volume_id")?;
        Ok(Volume(id))
    }

    pub fn id(&self) -> i64 {
        self.0
    }

    pub async fn writer_set(
        &self,
        conn: &mut AnyConnection,
        writer: Option<&Uuid>,
    ) -> Result<(), VolumeError> {
        query("UPDATE storage_volume SET volume_writer = ? WHERE volume_id = ?")
            .bind(writer.map(|w| w.to_string()))
            .bind(self.0)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn account_set(
        &self,
        conn: &mut AnyConnection,
        account: &Uuid,
    ) -> Result<(), VolumeError> {
        query("UPDATE storage_volume SET account_id = ? WHERE volume_id = ?")
            .bind(account.to_string())
            .bind(self.0)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn locked_set(
        &self,
        conn: &mut AnyConnection,
        locked: bool,
    ) -> Result<(), VolumeError> {
        query("UPDATE storage_volume SET volume_locked = ? WHERE volume_id = ?")
            .bind(locked)
            .bind(self.0)
            .execute(conn)
            .await?;
        Ok(())
    }
}

#[tokio::test]
async fn test_volume() {
    use fractal_storage_client::Privkey;
    use sqlx::AnyPool;

    let pool = AnyPool::connect("sqlite://:memory:").await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();

    let account = Uuid::new_v4();
    let privkey = Privkey::generate();
    let pubkey = privkey.pubkey();

    Volume::create(&mut conn, &pubkey, &account).await.unwrap();
    let volume = Volume::lookup(&mut conn, &pubkey).await.unwrap().unwrap();

    assert_eq!(volume.pubkey(), &pubkey);
    assert_eq!(volume.account(), &account);
}
