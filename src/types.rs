use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub generation: u64,
    pub parent: Option<u64>,
    pub creation: u64,
    pub size: u64,
}
