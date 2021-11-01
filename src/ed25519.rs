use bytes::Bytes;
use ed25519_dalek::{Digest, ExpandedSecretKey, PublicKey, SecretKey, Sha512, SIGNATURE_LENGTH};
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use rand_core::OsRng;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use std::pin::Pin;
use std::str::FromStr;
use std::error::Error as StdError;

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

/// Given a public key and a signed Ed25519 stream, this stream adaptor will
/// verify the stream on-the-fly.
pub struct VerifyStream<E: StdError> {
    pubkey: Pubkey,
    hasher: Sha512,
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    verification: Option<bool>,
    buffer: AllocRingBuffer<u8>,
}

impl<E: StdError> VerifyStream<E> {
    /// Create a new VerifyStream instance from an existing public key and stream.
    pub fn new(pubkey: &Pubkey, stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>) -> VerifyStream<E> {
        VerifyStream {
            pubkey: pubkey.clone(),
            hasher: Sha512::new(),
            stream,
            verification: None,
            buffer: AllocRingBuffer::with_capacity(SIGNATURE_LENGTH),
        }
    }

    /// Check to see if the stream is verified yet.
    pub fn verify(&self) -> Option<bool> {
        self.verification
    }
}

impl<E: StdError> Stream for VerifyStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // if the verification is done, stop passing through data
        if self.verification.is_some() {
            return Poll::Ready(None);
        }

        let result = Pin::new(&mut self.stream).poll_next(cx);
        match &result {
            Poll::Ready(Some(Ok(bytes))) => {
                // put stuff into ringbuffer
            }
            Poll::Ready(Some(Err(error))) => self.verification = Some(false),
            Poll::Ready(None) => {
                // do validation
            }
            _ => {}
        }

        result
    }
}
