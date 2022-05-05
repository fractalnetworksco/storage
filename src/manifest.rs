use crate::keys::{Privkey, Pubkey, Secret};
use anyhow::Result;
use ed25519_dalek_fiat::{ExpandedSecretKey, PublicKey, SecretKey, Signature, Verifier};
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;

pub const SNAPSHOT_HEADER_SIZE: usize = 3 * 8;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SnapshotManifest {
    /// Time that this snapshot was created.
    pub creation: u64,
    /// Machine that this snapshot was created on.
    pub machine: Uuid,
    /// Size of this snapshot, in bytes.
    pub size: u64,
    /// Size of this snapshot and the previous ones.
    pub size_total: u64,
    /// Volume of parent snapshot (if different).
    pub parent_volume: Option<Pubkey>,
    /// Hash of parent snapshot (if not root snapshot).
    pub parent_hash: Option<Vec<u8>>,
    /// Decryption key of parent snapshot (if needed), xored with current decryption key.
    pub parent_key: Option<Secret>,
    /// IPFS CID of data.
    pub data: Url,
}

impl SnapshotManifest {
    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub fn decode(data: &[u8]) -> Result<SnapshotManifest, Box<bincode::ErrorKind>> {
        bincode::deserialize(data)
    }

    pub fn sign(manifest: &[u8], privkey: &Privkey) -> Vec<u8> {
        let secret_key = SecretKey::from_bytes(privkey.as_slice()).unwrap();
        let public_key: PublicKey = (&secret_key).into();
        let secret_key: ExpandedSecretKey = (&secret_key).into();

        let signature = secret_key.sign(manifest, &public_key);
        let signature = signature.to_bytes().to_vec();
        signature
    }

    pub fn validate(manifest: &[u8], signature: &[u8], pubkey: &Pubkey) -> Result<()> {
        let pubkey = PublicKey::from_bytes(pubkey.as_slice())?;
        let signature = Signature::from_bytes(signature)?;
        pubkey.verify(manifest, &signature)?;
        Ok(())
    }
}

#[test]
fn manifest_encode_decode() {
    let manifest = SnapshotManifest {
        creation: 124123,
        machine: Uuid::new_v4(),
        size: 123412,
        size_total: 12341241,
        parent_volume: None,
        parent_hash: None,
        parent_key: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };
    let encoded = manifest.encode();
    let decoded = SnapshotManifest::decode(&encoded).unwrap();
    assert_eq!(manifest, decoded);
}

#[test]
fn manifest_sign_and_verify() {
    let privkey = Privkey::generate();
    let manifest = SnapshotManifest {
        creation: 124123,
        machine: Uuid::new_v4(),
        size: 123412,
        size_total: 12341241,
        parent_volume: None,
        parent_hash: None,
        parent_key: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };

    let encoded = manifest.encode();
    let signature = SnapshotManifest::sign(&encoded, &privkey);
    let validated = SnapshotManifest::validate(&encoded, &signature, &privkey.pubkey());
    assert!(validated.is_ok());
}
