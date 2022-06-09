use crate::keys::{Privkey, Pubkey, Secret};
use crate::Hash;
use anyhow::Result;
use ed25519_dalek_fiat::{ExpandedSecretKey, PublicKey, SecretKey, Signature, Verifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use url::Url;
use uuid::Uuid;

pub const MANIFEST_SIGNATURE_LENGTH: usize = 64;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Parent {
    hash: Vec<u8>,
    volume: Option<(Pubkey, Secret)>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// Time that this snapshot was created.
    pub creation: u64,
    /// Machine that this snapshot was created on.
    pub machine: Uuid,
    /// Size of this snapshot, in bytes.
    pub size: u64,
    /// Size of this snapshot and the previous ones.
    pub size_total: u64,
    /// Parent snapshot (if exists).
    #[serde(default)]
    pub parent: Option<Parent>,
    /// IPFS CID of data.
    pub data: Url,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ManifestSigned {
    pub manifest: Manifest,
    pub signature: Vec<u8>,
}

impl ManifestSigned {
    pub fn parse(from: &[u8]) -> Result<Self> {
        if let Some((manifest, signature)) = Manifest::split(from) {
            let manifest = Manifest::decode(manifest)?;
            let signature = signature.to_vec();
            Ok(ManifestSigned {
                manifest,
                signature,
            })
        } else {
            unimplemented!()
        }
    }
}

impl Manifest {
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
        size: 123412,
        size_total: 12341241,
        parent: None,
        data: "ipfs://QmTvXmLGiTV6CoCRvSEMHEKU3oMWsrVSMdhyKGzw9UcAth"
            .try_into()
            .unwrap(),
    };
    let manifest = manifest.encode();
    assert_eq!(Manifest::hash(&manifest).to_hex(), "8a0074e5f4c55b033b661c0851e0db6b19dc4ba97632586b2de72107168e0078c05f585b2418cc2e1d234388001a4f50926b98e22a19731260031a3a16ab0a40");
}

#[test]
fn manifest_encode_decode() {
    let manifest = Manifest {
        creation: 124123,
        machine: Uuid::new_v4(),
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
