use bytes::Bytes;
use ed25519_dalek::{Digest, ExpandedSecretKey, PublicKey, SecretKey, Sha512};
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use rand_core::OsRng;
use std::pin::Pin;
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

/// This SignedStream wraps around an existing Stream of Bytes, passing through
/// all of the data, but with the twist that if no error has occured while
/// streaming the data, it will append a valid Ed25519 Signature of the entire
/// data stream generated with the private key that it posesses.
pub struct SignedStream {
    privkey: Privkey,
    hasher: Sha512,
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync>>,
    eof: bool,
}

impl SignedStream {
    /// Create a new SignedStream instance, giving it a private key (this will
    /// be copied and stored) and a pinned, boxed Stream instance.
    pub fn new(
        privkey: &Privkey,
        stream: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync>>,
    ) -> Self {
        SignedStream {
            hasher: Sha512::new(),
            eof: false,
            privkey: privkey.clone(),
            stream,
        }
    }
}

impl Stream for SignedStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.eof {
            return Poll::Ready(None);
        }

        let result = Pin::new(&mut self.stream).poll_next(cx);
        match &result {
            Poll::Ready(Some(Ok(bytes))) => {
                self.hasher.update(bytes);
            }
            Poll::Ready(Some(Err(error))) => self.eof = true,
            Poll::Ready(None) => {
                self.eof = true;
                let secret_key = SecretKey::from_bytes(&self.privkey.0).unwrap();
                let public_key: PublicKey = (&secret_key).into();
                let secret_key: ExpandedSecretKey = (&secret_key).into();
                let result = secret_key.sign_prehashed(self.hasher.clone(), &public_key, None);
                match result {
                    Ok(signature) => {
                        return Poll::Ready(Some(Ok(Bytes::from(signature.to_bytes().to_vec()))))
                    }
                    Err(error) => unimplemented!(),
                }
            }
            _ => {}
        }

        result
    }
}

struct VerifyStream {
    pubkey: Pubkey,
}
