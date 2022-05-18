#[macro_use]
mod macros;

use blake2::{Blake2s256, Digest as Blake2Digest};
use ed25519_dalek_fiat::{PublicKey, SecretKey};
use paste::paste;
use rand_core::{OsRng, RngCore};
#[cfg(feature = "rocket")]
use rocket::request::FromParam;
#[cfg(feature = "schema")]
use schemars::JsonSchema;
use serde::{
    de::{Error, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
};
use serde_big_array::BigArray;
use sha2::Sha512;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;
use zeroize::Zeroize;

/// Possible errors that can be generated when parsing WireGuard keys.
#[derive(Error, Debug)]
pub enum ParseError {
    /// Error decoding base64
    #[cfg(feature = "base64")]
    #[error("base64 decoding error")]
    Base64(#[from] base64::DecodeError),
    /// Error decoding hex
    #[cfg(feature = "hex")]
    #[error("hex decoding errro")]
    Hex(#[from] hex::FromHexError),
    /// Error decoding base32
    #[cfg(feature = "base32")]
    #[error("base32 decoding error")]
    Base32Error,
    /// Illegal length
    #[error("length mismatch")]
    Length,
}

/// Length (in bytes) of an ed25519 public key.
pub const PUBKEY_LEN: usize = 32;

/// Length (in bytes) of a ed25519 private key.
pub const PRIVKEY_LEN: usize = 32;

/// Length (in bytes) of a preshared key.
pub const SECRET_LEN: usize = 32;

/// Length (in bytes) of a sha256 hash digest.
pub const HASH_LEN: usize = 64;

/// ed25519 public key.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Zeroize)]
pub struct Pubkey([u8; PUBKEY_LEN]);

impl_new!(Pubkey, PUBKEY_LEN);
impl_display!(Pubkey);
impl_deref!(Pubkey, PUBKEY_LEN);
#[cfg(feature = "hex")]
impl_hex!(Pubkey);
#[cfg(feature = "base64")]
impl_base64!(Pubkey);
#[cfg(feature = "base32")]
impl_base32!(Pubkey);
impl_parse!(Pubkey, PUBKEY_LEN);
impl_serde!(Pubkey, PUBKEY_LEN, "WireGuard public key");
#[cfg(feature = "rocket")]
impl_rocket!(Pubkey);

impl Pubkey {
    #[cfg(test)]
    fn test_generate() -> Pubkey {
        Privkey::generate().pubkey()
    }
}

#[test]
fn test_pubkey_from_slice() {
    let slice = [0; 3];
    match Pubkey::try_from(&slice[..]) {
        Err(ParseError::Length) => {}
        _ => assert!(false),
    }
    let slice = [0; PUBKEY_LEN];
    match Pubkey::try_from(&slice[..]) {
        Ok(_) => {}
        _ => assert!(false),
    }
}

impl TryFrom<&[u8]> for Pubkey {
    type Error = ParseError;
    fn try_from(key: &[u8]) -> Result<Self, Self::Error> {
        if key.len() != PUBKEY_LEN {
            Err(ParseError::Length)
        } else {
            let mut data = [0; PUBKEY_LEN];
            data[0..PUBKEY_LEN].copy_from_slice(&key[0..PUBKEY_LEN]);
            Ok(Pubkey(data))
        }
    }
}

/// WireGuard private key.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Zeroize)]
pub struct Privkey([u8; PRIVKEY_LEN]);

impl_display!(Privkey);
impl_new!(Privkey, PRIVKEY_LEN);
impl_deref!(Privkey, PRIVKEY_LEN);
#[cfg(feature = "hex")]
impl_hex!(Privkey);
#[cfg(feature = "base64")]
impl_base64!(Privkey);
#[cfg(feature = "base32")]
impl_base32!(Privkey);
impl_parse!(Privkey, PRIVKEY_LEN);
impl_serde!(Privkey, PRIVKEY_LEN, "WireGuard private key");
#[cfg(feature = "rocket")]
impl_rocket!(Privkey);

impl Privkey {
    /// Generate new private key using the kernel randomness generator.
    pub fn generate() -> Self {
        let private_key = SecretKey::generate(&mut OsRng);
        Privkey(private_key.to_bytes())
    }

    #[cfg(test)]
    pub fn test_generate() -> Self {
        Self::generate()
    }

    /// Generate the corresponding public key for this private key.
    pub fn pubkey(&self) -> Pubkey {
        let private_key = SecretKey::from_bytes(&self.0).unwrap();
        let public_key: PublicKey = (&private_key).into();
        Pubkey(public_key.to_bytes())
    }

    /// Derive secret by hashing with blake2.
    pub fn derive_secret(&self) -> Secret {
        let mut hasher = Blake2s256::new();
        hasher.update(self.as_slice());
        let output = hasher.finalize();
        Secret(output.as_slice().try_into().unwrap())
    }
}

#[test]
fn test_privkey_to_secret() {
    let privkey = Privkey::generate();
    let _secret = privkey.derive_secret();

    let privkey = Privkey::from_str("CHmZHrfC5uRMUs3J7qjmc4dl+32f157mfLdV9b5Ca2o=").unwrap();
    let secret = privkey.derive_secret();
    assert_eq!(
        &secret.to_base64(),
        "f1loUq2/FQVkW/ytvYcdwSRU2o/djxU+6nfW0YxHs/4="
    );
}

#[test]
fn test_privkey_from_slice() {
    let slice = [0; 3];
    match Privkey::try_from(&slice[..]) {
        Err(ParseError::Length) => {}
        _ => assert!(false),
    }
    let slice = [0; PRIVKEY_LEN];
    match Privkey::try_from(&slice[..]) {
        Ok(_) => {}
        _ => assert!(false),
    }
}

impl TryFrom<&[u8]> for Privkey {
    type Error = ParseError;
    fn try_from(key: &[u8]) -> Result<Self, Self::Error> {
        if key.len() != PUBKEY_LEN {
            Err(ParseError::Length)
        } else {
            let mut data = [0; PUBKEY_LEN];
            data[0..PUBKEY_LEN].copy_from_slice(&key[0..PUBKEY_LEN]);
            Ok(Privkey(data))
        }
    }
}

#[test]
fn test_storage_privkey() {
    let key = Privkey::generate();
    // always generate same pubkey
    assert_eq!(key.pubkey(), key.pubkey());
}

/// WireGuard preshared key.
#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Zeroize)]
pub struct Secret([u8; SECRET_LEN]);

impl_new!(Secret, SECRET_LEN);
impl_display!(Secret);
impl_deref!(Secret, SECRET_LEN);
#[cfg(feature = "hex")]
impl_hex!(Secret);
#[cfg(feature = "base64")]
impl_base64!(Secret);
#[cfg(feature = "base32")]
impl_base32!(Secret);
impl_parse!(Secret, SECRET_LEN);
impl_serde!(Secret, SECRET_LEN, "WireGuard preshared key");
#[cfg(feature = "rocket")]
impl_rocket!(Secret);

impl Secret {
    /// Generate new random preshared key using the system randomness generator.
    pub fn generate() -> Self {
        let mut data = [0; SECRET_LEN];
        OsRng.fill_bytes(&mut data);
        Secret(data)
    }

    #[cfg(test)]
    pub fn test_generate() -> Self {
        Self::generate()
    }
}

#[test]
fn test_secret_from_slice() {
    let slice = [0; 3];
    match Secret::try_from(&slice[..]) {
        Err(ParseError::Length) => {}
        _ => assert!(false),
    }
    let slice = [0; PRIVKEY_LEN];
    match Secret::try_from(&slice[..]) {
        Ok(_) => {}
        _ => assert!(false),
    }
}

impl TryFrom<&[u8]> for Secret {
    type Error = ParseError;
    fn try_from(key: &[u8]) -> Result<Self, Self::Error> {
        if key.len() != PUBKEY_LEN {
            Err(ParseError::Length)
        } else {
            let mut data = [0; PUBKEY_LEN];
            data[0..PUBKEY_LEN].copy_from_slice(&key[0..PUBKEY_LEN]);
            Ok(Secret(data))
        }
    }
}

#[cfg_attr(feature = "schema", derive(JsonSchema))]
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Zeroize)]
pub struct Hash([u8; HASH_LEN]);

impl_new!(Hash, HASH_LEN);
impl_display!(Hash);
impl_deref!(Hash, HASH_LEN);
#[cfg(feature = "hex")]
impl_hex!(Hash);
#[cfg(feature = "base64")]
impl_base64!(Hash);
#[cfg(feature = "base32")]
impl_base32!(Hash);
impl_parse!(Hash, HASH_LEN);
impl_serde!(Hash, HASH_LEN, "Sha256 hash sum");
#[cfg(feature = "rocket")]
impl_rocket!(Hash);

impl Hash {
    pub fn generate(data: &[u8]) -> Self {
        let mut hasher = Sha512::new();
        hasher.update(data);
        let hash = hasher.finalize();
        let hash_ref: &[u8; 64] = hash.as_ref();
        Hash(hash_ref.clone())
    }

    #[cfg(test)]
    pub fn test_generate() -> Self {
        let mut data = [0; HASH_LEN];
        OsRng.fill_bytes(&mut data);
        Hash(data)
    }
}

#[test]
fn test_hash_from_slice() {
    let slice = [0; 3];
    match Hash::try_from(&slice[..]) {
        Err(ParseError::Length) => {}
        _ => assert!(false),
    }
    let slice = [0; HASH_LEN];
    match Hash::try_from(&slice[..]) {
        Ok(_) => {}
        _ => assert!(false),
    }
}

impl TryFrom<&[u8]> for Hash {
    type Error = ParseError;
    fn try_from(key: &[u8]) -> Result<Self, Self::Error> {
        if key.len() != HASH_LEN {
            Err(ParseError::Length)
        } else {
            let mut data = [0; HASH_LEN];
            data[0..HASH_LEN].copy_from_slice(&key[0..HASH_LEN]);
            Ok(Hash(data))
        }
    }
}
