use ed25519_dalek::{PublicKey, SecretKey};
use rand_core::OsRng;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct Privkey([u8; 32]);

#[derive(Clone, Copy, Debug)]
pub struct Pubkey([u8; 32]);

impl Pubkey {
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl Privkey {
    pub fn generate() -> Self {
        let secret_key = SecretKey::generate(&mut OsRng);
        Privkey(secret_key.to_bytes())
    }

    pub fn pubkey(&self) -> Pubkey {
        let secret_key = SecretKey::from_bytes(&self.0).unwrap();
        let public_key: PublicKey = (&secret_key).into();
        Pubkey(public_key.to_bytes())
    }
}

impl FromStr for Privkey {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let data = base64::decode(s)?;
        Ok(Privkey(data.try_into().unwrap()))
    }
}

impl std::fmt::Display for Privkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}

impl std::fmt::Display for Pubkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", base64::encode(&self.0))
    }
}
