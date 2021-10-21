
#[derive(Clone, Debug)]
pub struct SnapshotInfo {
    pub generation: usize,
    pub parent: Option<usize>,
    pub creation: Option<String>,
}
