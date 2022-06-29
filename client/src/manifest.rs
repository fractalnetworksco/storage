use crate::keys::{Privkey, Pubkey, Secret};
use crate::Hash;
use anyhow::Result;
use ed25519_dalek_fiat::{ExpandedSecretKey, PublicKey, SecretKey, Signature, Verifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::path::PathBuf;
#[cfg(test)]
use std::str::FromStr;
use url::Url;
use uuid::Uuid;

pub const MANIFEST_SIGNATURE_LENGTH: usize = 64;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Parent {
    /// Hash of parent snapshot.
    pub hash: Hash,
    /// If the parent snapshot is not in the same volume, this has the pubkey and secret needed to
    /// look it up.
    pub volume: Option<(Pubkey, Secret)>,
}

impl Parent {
    /// Initialize new parent with given hash
    pub fn new(hash: Hash) -> Self {
        Parent { hash, volume: None }
    }

    /// Given a parent, add a volume
    pub fn with_volume(self, pubkey: Pubkey, secret: Secret) -> Self {
        Parent {
            volume: Some((pubkey, secret)),
            hash: self.hash,
        }
    }
}

/// Manifest for snapshot.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Time that this snapshot was created.
    pub creation: u64,
    /// Machine that this snapshot was created on.
    pub machine: Uuid,
    /// Path to snapshot
    pub path: PathBuf,
    /// Size of this snapshot, in bytes.
    pub size: u64,
    /// Size of this snapshot and the previous ones.
    pub size_total: u64,
    /// Generation count, monotonically incremented.
    pub generation: u64,
    /// Parent snapshot (if exists).
    #[serde(default)]
    pub parent: Option<Parent>,
    /// IPFS CID of data.
    pub data: Url,
}

/// Signed manifest, keeps raw encoded data, decoded manifest, and raw signature.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ManifestSigned {
    pub raw: Vec<u8>,
    pub manifest: Manifest,
    pub signature: Vec<u8>,
}

#[derive(thiserror::Error, Debug)]
pub enum ManifestSignedParseError {
    #[error("Error decoding bincode: {0:}")]
    Bincode(#[from] Box<bincode::ErrorKind>),
    #[error("Missing snapshot signature, got length {0:}, expected {MANIFEST_SIGNATURE_LENGTH}")]
    MissingSignature(usize),
}

impl ManifestSigned {
    /// Try parsing signed manifest from combined data.
    pub fn parse(from: &[u8]) -> Result<Self, ManifestSignedParseError> {
        if let Some((manifest, signature)) = Manifest::split(from) {
            Ok(Self::from_parts(manifest, signature)?)
        } else {
            Err(ManifestSignedParseError::MissingSignature(from.len()))
        }
    }

    /// Try parsing signed manifest from parts.
    pub fn from_parts(manifest: &[u8], signature: &[u8]) -> Result<Self, Box<bincode::ErrorKind>> {
        let decoded = Manifest::decode(manifest)?;
        let signature = signature.to_vec();
        Ok(ManifestSigned {
            raw: manifest.to_vec(),
            manifest: decoded,
            signature,
        })
    }

    /// Return the raw data for this signature
    pub fn data(&self) -> Vec<u8> {
        self.raw
            .iter()
            .chain(self.signature.iter())
            .cloned()
            .collect()
    }

    /// Validate this signed manifest.
    pub fn validate(&self, pubkey: &Pubkey) -> Result<()> {
        Manifest::validate(&self.raw, &self.signature, pubkey)
    }

    /// Generate hash of manifest.
    pub fn hash(&self) -> Hash {
        Manifest::hash(&self.raw)
    }
}

impl Manifest {
    /// Given a manifest and a private key, produce a signed manifest.
    pub fn sign(&self, privkey: &Privkey) -> ManifestSigned {
        let encoded = self.encode();
        let signature = Self::signature(&encoded, privkey);
        ManifestSigned {
            raw: encoded,
            manifest: self.clone(),
            signature,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    pub fn decode(data: &[u8]) -> Result<Manifest, Box<bincode::ErrorKind>> {
        bincode::deserialize(data)
    }

    pub fn signature(manifest: &[u8], privkey: &Privkey) -> Vec<u8> {
        let secret_key = SecretKey::from_bytes(privkey.as_slice()).unwrap();
        let public_key: PublicKey = (&secret_key).into();
        let secret_key: ExpandedSecretKey = (&secret_key).into();

        let signature = secret_key.sign(manifest, &public_key);
        let signature = signature.to_bytes().to_vec();
        signature
    }

    pub fn hash(manifest: &[u8]) -> Hash {
        let mut hasher = Sha512::new();
        hasher.update(&manifest);
        let hash = hasher.finalize();
        Hash::try_from(hash.as_slice()).unwrap()
    }

    pub fn signed(&self, privkey: &Privkey) -> Vec<u8> {
        let mut encoded = self.encode();
        let mut signature = Self::signature(&encoded, privkey);
        encoded.append(&mut signature);
        encoded
    }

    pub fn validate(manifest: &[u8], signature: &[u8], pubkey: &Pubkey) -> Result<()> {
        let pubkey = PublicKey::from_bytes(pubkey.as_slice())?;
        let signature = Signature::from_bytes(signature)?;
        pubkey.verify(manifest, &signature)?;
        Ok(())
    }

    pub fn split(data: &[u8]) -> Option<(&[u8], &[u8])> {
        if data.len() < MANIFEST_SIGNATURE_LENGTH {
            return None;
        }

        Some((
            &data[0..data.len() - MANIFEST_SIGNATURE_LENGTH],
            &data[data.len() - MANIFEST_SIGNATURE_LENGTH..data.len()],
        ))
    }
}

#[test]
fn manifest_hash() {
    let manifest = Manifest {
        creation: 124123,
        machine: Uuid::default(),
        path: PathBuf::from_str("/tmp/path").unwrap(),
        generation: 0,
        size: 123412,
        size_total: 12341241,
        parent: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };
    let manifest = manifest.encode();
    assert_eq!(Manifest::hash(&manifest).to_hex(), "ab93233657a07df4bde570f9b2ad3d069e14fc80e5b07c3773a937d624b8f7bbf2dade0a3d48a121274e1fc8e787d72fd88171f10a66e84e4207a03d45acf637");
}

#[test]
fn manifest_encode_decode() {
    let manifest = Manifest {
        creation: 124123,
        generation: 0,
        machine: Uuid::new_v4(),
        path: PathBuf::from_str("/tmp/path").unwrap(),
        size: 123412,
        size_total: 12341241,
        parent: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };
    let encoded = manifest.encode();
    let decoded = Manifest::decode(&encoded).unwrap();
    assert_eq!(manifest, decoded);
}

#[test]
fn manifest_sign_and_verify() {
    let privkey = Privkey::generate();
    let manifest = Manifest {
        creation: 124123,
        machine: Uuid::new_v4(),
        path: PathBuf::from_str("/tmp/path").unwrap(),
        generation: 0,
        size: 123412,
        size_total: 12341241,
        parent: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };

    let encoded = manifest.encode();
    let signature = Manifest::signature(&encoded, &privkey);
    let validated = Manifest::validate(&encoded, &signature, &privkey.pubkey());
    assert!(validated.is_ok());
}

#[test]
fn manifest_sign_split() {
    let privkey = Privkey::generate();
    let manifest = Manifest {
        creation: 124123,
        machine: Uuid::new_v4(),
        size: 123412,
        path: PathBuf::from_str("/tmp/path").unwrap(),
        generation: 0,
        size_total: 12341241,
        parent: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };

    let data = manifest.signed(&privkey);
    let (encoded, signature) = Manifest::split(&data).unwrap();
    assert_eq!(encoded, manifest.encode());
    assert_eq!(signature, Manifest::signature(encoded, &privkey));
}
