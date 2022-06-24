use crate::volume::{Volume, VolumeData};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::any::AnyRow;
use sqlx::{query, AnyConnection, Row};
use storage_api::{Hash, Manifest, ManifestSigned};
use thiserror::Error;

/// Minimum accepted size for BTRFS snapshot. Experientally determined, used as safeguard
/// to prevent broken snapshots from being accepted.
pub const MINIMUM_SNAPSHOT_SIZE: u64 = 64;

#[derive(Error, Debug)]
pub enum SnapshotError {
    #[error("Manifest Invalid")]
    ManifestInvalid,
    #[error("Database error: {0:}")]
    Database(#[from] sqlx::Error),
    #[error("Missing rowid")]
    MissingRowid,
    #[error("Wrong size_total, expected {0:} but got {1:}")]
    WrongSizeTotal(u64, u64),
    #[error("Missing parent with hash {0:}")]
    MissingParent(Hash),
    #[error("Cannot decode manifest: {0:}")]
    ManifestDecode(String),
    #[error("Invalid generation: manifest has generation {0:} but parent has {1:}")]
    InvalidGeneration(u64, u64),
    #[error("Invalid size in manifest: {0:} (must be bigger than {MINIMUM_SNAPSHOT_SIZE} bytes)")]
    InvalidSize(u64),
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Snapshot(i64);

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotData {
    id: i64,
    volume: i64,
    parent: Option<i64>,
    manifest: ManifestSigned,
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
            manifest: ManifestSigned::from_parts(&manifest, &signature)
                .map_err(|e| SnapshotError::ManifestDecode(e.to_string()))?,
            hash,
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot(self.id)
    }

    pub fn manifest_signed(&self) -> &ManifestSigned {
        &self.manifest
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest.manifest
    }

    pub fn signature(&self) -> &[u8] {
        &self.manifest.signature
    }

    pub fn hash(&self) -> Hash {
        Hash::try_from(self.hash.as_slice()).unwrap()
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
        hash: &Hash,
        parent: Option<&Snapshot>,
        generation: u64,
    ) -> Result<Snapshot, SnapshotError> {
        let result = query(
            "INSERT INTO storage_snapshot(
            volume_id,
            snapshot_manifest,
            snapshot_signature,
            snapshot_hash,
            snapshot_parent,
            snapshot_generation)
            VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(volume.id())
        .bind(manifest)
        .bind(signature)
        .bind(hash.as_slice())
        .bind(parent.map(|p| p.id()))
        .bind(generation as i64)
        .execute(conn)
        .await?;
        Ok(Snapshot(
            result.last_insert_id().ok_or(SnapshotError::MissingRowid)?,
        ))
    }

    pub async fn create_from_manifest(
        conn: &mut AnyConnection,
        volume: &VolumeData,
        manifest: &[u8],
    ) -> Result<Snapshot, SnapshotError> {
        let (manifest, signature) =
            Manifest::split(&manifest).ok_or(SnapshotError::ManifestInvalid)?;
        Manifest::validate(manifest, signature, volume.pubkey()).unwrap();
        let parsed = Manifest::decode(manifest).map_err(|_| SnapshotError::ManifestInvalid)?;
        let hash = Manifest::hash(manifest);
        let parent = match &parsed.parent {
            Some(parent) if parent.volume.is_none() => {
                let parent = Snapshot::fetch_by_hash(conn, &volume.volume(), &parent.hash)
                    .await?
                    .ok_or_else(|| SnapshotError::MissingParent(parent.hash))?;
                let expected_size_total = parent.manifest().size_total + parsed.size;
                if parsed.size_total != expected_size_total {
                    return Err(SnapshotError::WrongSizeTotal(
                        parsed.size_total,
                        expected_size_total,
                    ));
                }
                if parsed.generation <= parent.manifest().generation {
                    return Err(SnapshotError::InvalidGeneration(
                        parsed.generation,
                        parent.manifest().generation,
                    ));
                }
                if parsed.size < MINIMUM_SNAPSHOT_SIZE {
                    return Err(SnapshotError::InvalidSize(parsed.size));
                }
                Some(parent.snapshot())
            }
            Some(_parent) => None,
            None => {
                if parsed.size != parsed.size_total {
                    return Err(SnapshotError::WrongSizeTotal(
                        parsed.size_total,
                        parsed.size,
                    ));
                }
                None
            }
        };

        let snapshot = Snapshot::create(
            conn,
            &volume.volume(),
            manifest,
            signature,
            &hash,
            parent.as_ref(),
            parsed.generation,
        )
        .await?;

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
        volume: &Volume,
        hash: &Hash,
    ) -> Result<Option<SnapshotData>, SnapshotError> {
        let row = query("SELECT * FROM storage_snapshot WHERE snapshot_hash = ? AND volume_id = ?")
            .bind(hash.as_slice())
            .bind(volume.id())
            .fetch_optional(conn)
            .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some(SnapshotData::from_row(&row)?)),
        }
    }

    pub async fn list(
        conn: &mut AnyConnection,
        volume: &Volume,
        parent: Option<&Snapshot>,
        root: bool,
    ) -> Result<Vec<SnapshotData>, SnapshotError> {
        let rows = query(
            "SELECT * FROM storage_snapshot
                WHERE volume_id = $1
                AND ($2 IS NULL OR snapshot_parent = $2)
                AND ($3 = 0 OR snapshot_parent IS NULL)",
        )
        .bind(volume.id() as i64)
        .bind(parent.map(|parent| parent.id()))
        .bind(root)
        .fetch_all(conn)
        .await
        .unwrap();
        let mut snapshots = vec![];
        for row in &rows {
            snapshots.push(SnapshotData::from_row(row)?);
        }
        Ok(snapshots)
    }
}

#[tokio::test]
async fn test_snapshot_create() {
    use sqlx::AnyPool;
    use storage_api::Privkey;
    use uuid::Uuid;

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

    let manifest = Manifest {
        creation: 0,
        data: "ipfs://asd99a0s8098da0sd98".parse().unwrap(),
        generation: 0,
        parent: None,
        size: MINIMUM_SNAPSHOT_SIZE,
        size_total: MINIMUM_SNAPSHOT_SIZE,
        machine: Default::default(),
        path: std::path::PathBuf::from("abc"),
    };
    let manifest_signed = manifest.sign(&privkey);
    let snapshot = Snapshot::create(
        &mut conn,
        &volume.volume(),
        &manifest_signed.raw,
        &manifest_signed.signature,
        &manifest_signed.hash(),
        None,
        0,
    )
    .await
    .unwrap();

    let snapshot_data = snapshot.fetch(&mut conn).await.unwrap();
    assert_eq!(snapshot_data.snapshot(), snapshot);
    assert_eq!(snapshot_data.manifest_signed().raw, manifest_signed.raw);
    assert_eq!(snapshot_data.signature(), manifest_signed.signature);
    assert_eq!(snapshot_data.hash(), manifest_signed.hash());

    let snapshot_data =
        Snapshot::fetch_by_hash(&mut conn, &volume.volume(), &manifest_signed.hash())
            .await
            .unwrap()
            .unwrap();
    assert_eq!(snapshot_data.snapshot(), snapshot);
    assert_eq!(snapshot_data.manifest_signed().raw, manifest_signed.raw);
    assert_eq!(snapshot_data.signature(), manifest_signed.signature);
    assert_eq!(snapshot_data.hash(), manifest_signed.hash());
}
