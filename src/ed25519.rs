use blake2::{Blake2s256, Digest as Blake2Digest};
use bytes::{Buf, Bytes, BytesMut};
use ed25519_dalek_fiat::{
    Digest, ExpandedSecretKey, PublicKey, SecretKey, Sha512, Signature, SIGNATURE_LENGTH,
};
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter, Result as FmtResult};
use std::pin::Pin;
use wireguard_keys::{Privkey, Pubkey};

pub trait ToChaCha20 {
    fn to_chacha20_key(&self) -> chacha20::Key;
}

impl ToChaCha20 for Privkey {
    fn to_chacha20_key(&self) -> chacha20::Key {
        let mut hasher = Blake2s256::new();
        hasher.update(self.as_slice());
        let output = hasher.finalize();
        chacha20::Key::clone_from_slice(&output)
    }
}

/// This SignStream wraps around an existing Stream of Bytes, passing through
/// all of the data, but with the twist that if no error has occured while
/// streaming the data, it will append a valid Ed25519 Signature of the entire
/// data stream generated with the private key that it posesses.
pub struct SignStream<E: StdError> {
    privkey: Privkey,
    hasher: Sha512,
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    eof: bool,
}

impl<E: StdError> SignStream<E> {
    /// Create a new SignStream instance, giving it a private key (this will
    /// be copied and stored) and a pinned, boxed Stream instance.
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static>(
        stream: S,
        privkey: &Privkey,
    ) -> Self {
        SignStream {
            hasher: Sha512::new(),
            eof: false,
            privkey: privkey.clone(),
            stream: Box::pin(stream),
        }
    }
}

impl<E: StdError> Stream for SignStream<E> {
    type Item = Result<Bytes, E>;

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
                let secret_key = SecretKey::from_bytes(self.privkey.as_slice()).unwrap();
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
    state: VerifyStreamState,
}

#[derive(Clone, Debug)]
pub enum VerifyError<E: StdError> {
    Stream(E),
    Incorrect,
}

impl<E: StdError> Display for VerifyError<E> {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        use VerifyError::*;
        match self {
            Stream(err) => write!(f, "{}", err),
            Incorrect => write!(f, "ed25519 signature validation incorrect"),
        }
    }
}

impl<E: StdError> StdError for VerifyError<E> {}

pub enum VerifyStreamState {
    Start(Sha512, BytesMut),
    Valid,
    Invalid,
    Error,
}

impl<E: StdError> VerifyStream<E> {
    /// Create a new VerifyStream instance from an existing public key and stream.
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static>(
        pubkey: &Pubkey,
        stream: S,
    ) -> VerifyStream<E> {
        VerifyStream {
            pubkey: pubkey.clone(),
            hasher: Sha512::new(),
            stream: Box::pin(stream),
            verification: None,
            buffer: BytesMut::with_capacity(SIGNATURE_LENGTH),
            queue: None,
            state: VerifyStreamState::Start(
                Sha512::new(),
                BytesMut::with_capacity(SIGNATURE_LENGTH),
            ),
        }
    }

    /// Check to see if the stream is verified yet.
    pub fn verify(&self) -> Option<bool> {
        self.verification
    }
}

impl<E: StdError> Stream for VerifyStream<E> {
    type Item = Result<Bytes, VerifyError<E>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // if the verification is done, stop passing through data
        if self.verification.is_some() {
            return Poll::Ready(None);
        }

        if let Some(queue) = self.queue.clone() {
            self.queue = None;
            return Poll::Ready(Some(Ok(queue)));
        }

        let result = Pin::new(&mut self.stream).poll_next(cx);
        match result {
            Poll::Ready(Some(Ok(mut bytes))) => {
                // if we haven't gotten a full signature yet, just read and keep pending.
                let total_length = self.buffer.len() + bytes.len();
                if total_length <= SIGNATURE_LENGTH {
                    // we have nothing to return yet.
                    self.buffer.extend_from_slice(&bytes);
                    return Poll::Ready(Some(Ok(Bytes::new())));
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
                    if bytes.len() > 0 {
                        self.queue = Some(bytes.clone());
                    }

                    // hash new data
                    self.hasher.update(&retval);
                    self.hasher.update(&bytes);

                    // return previous buffer
                    Poll::Ready(Some(Ok(retval)))
                } else {
                    let mut retval = self.buffer.clone();
                    let buffer_fragment = retval.split_off(done_bytes);
                    self.buffer.clear();
                    self.buffer.extend_from_slice(&buffer_fragment);
                    self.buffer.extend_from_slice(&bytes);

                    self.hasher.update(&retval);
                    Poll::Ready(Some(Ok(retval.freeze())))
                }
            }
            Poll::Ready(Some(Err(error))) => {
                self.verification = Some(false);
                Poll::Ready(Some(Err(VerifyError::Stream(error))))
            }
            Poll::Ready(None) => {
                if self.buffer.len() < SIGNATURE_LENGTH {
                    self.verification = Some(false);
                    return Poll::Ready(Some(Err(VerifyError::Incorrect)));
                }

                let mut signature = [0; SIGNATURE_LENGTH];
                self.buffer.copy_to_slice(&mut signature);
                let signature = match Signature::from_bytes(&signature) {
                    Ok(signature) => signature,
                    Err(_) => {
                        self.verification = Some(false);
                        return Poll::Ready(Some(Err(VerifyError::Incorrect)));
                    }
                };

                let pubkey = PublicKey::from_bytes(self.pubkey.as_slice()).unwrap();
                let result = pubkey
                    .verify_prehashed(self.hasher.clone(), None, &signature)
                    .is_ok();
                self.verification = Some(result);
                if !result {
                    Poll::Ready(Some(Err(VerifyError::Incorrect)))
                } else {
                    Poll::Ready(None)
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
#[tokio::test]
async fn sign_empty_stream() {
    use futures::StreamExt;
    let key = Privkey::generate();
    let stream = futures::stream::iter(vec![]);
    let mut stream = SignStream::<std::io::Error>::new(stream, &key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 64);

    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn sign_single_stream() {
    use futures::StreamExt;
    let key = Privkey::generate();
    let data: Bytes = "this is some test data".into();
    let stream = futures::stream::iter(vec![Ok(data.clone())]);
    let mut stream = SignStream::<std::io::Error>::new(stream, &key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 64);

    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn sign_multi_stream() {
    use futures::StreamExt;
    let key = Privkey::generate();
    let data1: Bytes = "this is some test data".into();
    let data2: Bytes = "hello world".into();
    let data3: Bytes = "oj is guilty".into();
    let stream = futures::stream::iter(vec![
        Ok(data1.clone()),
        Ok(data2.clone()),
        Ok(data3.clone()),
    ]);
    let mut stream = SignStream::<std::io::Error>::new(stream, &key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data1);
    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data2);
    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data3);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 64);

    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn sign_error_stream() {
    use futures::StreamExt;
    let key = Privkey::generate();
    let data1: Bytes = "this is some test data".into();
    let data2: Bytes = "the answer is 42".into();
    let stream = futures::stream::iter(vec![
        Ok(data1.clone()),
        Err(std::io::Error::new(std::io::ErrorKind::Other, "error")),
        Ok(data2.clone()),
    ]);
    let mut stream = SignStream::<std::io::Error>::new(stream, &key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data1);
    let result = stream.next().await.unwrap();
    assert!(result.is_err());

    // do not produce signature after error
    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn verify_empty_stream() {
    use futures::StreamExt;
    let key = Privkey::generate().pubkey();
    let stream = futures::stream::iter(vec![]);
    let mut stream = VerifyStream::<std::io::Error>::new(&key, Box::pin(stream));

    let result = stream.next().await.unwrap();
    assert!(result.is_err());

    assert!(stream.next().await.is_none());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn verify_missing_stream() {
    use futures::StreamExt;
    let key = Privkey::generate().pubkey();
    let data1: Bytes = "this is some short test".into();
    let data2: Bytes = "data that is used to assess".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let mut stream = VerifyStream::<std::io::Error>::new(&key, Box::pin(stream));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    assert!(stream.next().await.unwrap().is_err());
    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn verify_correct_stream() {
    use futures::StreamExt;
    let privkey = Privkey::generate();
    let pubkey = privkey.pubkey();

    let data1: Bytes = "this is some short test".into();
    let data2: Bytes = "data that is used to assess".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let stream = SignStream::<std::io::Error>::new(stream, &privkey);
    let mut stream = VerifyStream::<std::io::Error>::new(&pubkey, Box::pin(stream));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);
    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), data1.len() + data2.len());

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn verify_incorrect_stream() {
    use futures::StreamExt;
    let privkey = Privkey::generate();
    let pubkey = Privkey::generate().pubkey();

    let data1: Bytes = "this is some short test".into();
    let data2: Bytes = "data that is used to assess".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let stream = SignStream::<std::io::Error>::new(stream, &privkey);
    let mut stream = VerifyStream::<std::io::Error>::new(&pubkey, Box::pin(stream));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);
    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), data1.len() + data2.len());

    // error because signature is invalid
    let result = stream.next().await.unwrap();
    assert!(result.is_err());

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn verify_corrupt_stream() {
    use futures::StreamExt;
    let privkey = Privkey::generate();
    let pubkey = privkey.pubkey();

    let data1: Bytes = "this is some short test".into();
    let data2: Bytes = "data that is used to assess".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let mut stream = SignStream::<std::io::Error>::new(stream, &privkey);
    let mut data = vec![];
    while let Some(item) = stream.next().await {
        if data.len() > 0 {
            data.push(item);
        } else {
            // corrupt some data
            let mut item: BytesMut = item.unwrap().chunk().into();
            item[0] = 56;
            data.push(Ok(item.freeze()));
        }
    }
    let stream = futures::stream::iter(data);
    let mut stream = VerifyStream::<std::io::Error>::new(&pubkey, Box::pin(stream));

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);
    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 0);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), data1.len() + data2.len());

    // error because signature is invalid
    let result = stream.next().await.unwrap();
    assert!(result.is_err());

    assert!(stream.next().await.is_none());
}
