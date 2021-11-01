use bytes::{Buf, Bytes, BytesMut};
use ed25519_dalek::{
    Digest, ExpandedSecretKey, PublicKey, SecretKey, Sha512, Signature, SIGNATURE_LENGTH,
};
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use rand_core::OsRng;
use std::error::Error as StdError;
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

/// Given a public key and a signed Ed25519 stream, this stream adaptor will
/// verify the stream on-the-fly.
pub struct VerifyStream<E: StdError> {
    pubkey: Pubkey,
    hasher: Sha512,
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    verification: Option<bool>,
    buffer: BytesMut,
    queue: Option<Bytes>,
}

impl<E: StdError> VerifyStream<E> {
    /// Create a new VerifyStream instance from an existing public key and stream.
    pub fn new(
        pubkey: &Pubkey,
        stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    ) -> VerifyStream<E> {
        VerifyStream {
            pubkey: pubkey.clone(),
            hasher: Sha512::new(),
            stream,
            verification: None,
            buffer: BytesMut::with_capacity(SIGNATURE_LENGTH),
            queue: None,
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

        if let Some(queue) = self.queue.clone() {
            self.queue = None;
            return Poll::Ready(Some(Ok(queue)));
        }

        let mut result = Pin::new(&mut self.stream).poll_next(cx);
        match &mut result {
            Poll::Ready(Some(Ok(bytes))) => {
                // if we haven't gotten a full signature yet, just read and keep pending.
                let total_length = self.buffer.len() + bytes.len();
                if total_length <= SIGNATURE_LENGTH {
                    // we have nothing to return yet.
                    self.buffer.extend_from_slice(bytes);
                    return Poll::Pending;
                }

                // how many bytes are ready to return?
                let done_bytes = total_length - SIGNATURE_LENGTH;

                // do we return the entire buffer?
                if done_bytes >= self.buffer.len() {
                    let retval = self.buffer.clone().freeze();

                    // split off new buffer
                    let new_buffer = bytes.split_off(bytes.len() - SIGNATURE_LENGTH);
                    self.buffer.clear();
                    self.buffer.extend_from_slice(&new_buffer);

                    // update queue
                    self.queue = Some(bytes.clone());

                    // hash new data
                    self.hasher.update(&retval);
                    self.hasher.update(&bytes);

                    // return previous buffer
                    return Poll::Ready(Some(Ok(retval)));
                } else {
                    let mut retval = self.buffer.clone();
                    let buffer_fragment = retval.split_off(done_bytes);
                    self.buffer.clear();
                    self.buffer.extend_from_slice(&buffer_fragment);
                    self.buffer.extend_from_slice(&bytes);

                    self.hasher.update(&retval);
                    return Poll::Ready(Some(Ok(retval.freeze())));
                }
            }
            Poll::Ready(Some(Err(error))) => self.verification = Some(false),
            Poll::Ready(None) => {
                if self.buffer.len() < SIGNATURE_LENGTH {
                    self.verification = Some(false);
                    return Poll::Ready(None);
                }

                let mut signature = [0; SIGNATURE_LENGTH];
                self.buffer.copy_to_slice(&mut signature);
                let signature = Signature::new(signature);

                let pubkey = PublicKey::from_bytes(&self.pubkey.0).unwrap();
                let result = pubkey
                    .verify_prehashed(self.hasher.clone(), None, &signature)
                    .is_ok();
                self.verification = Some(result);
            }
            _ => {}
        }

        result
    }
}
