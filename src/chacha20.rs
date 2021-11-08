use bytes::Bytes;
use chacha20::cipher::{NewCipher, StreamCipher};
use chacha20::{Key, XChaCha20, XNonce};
use futures::task::Context;
use futures::task::Poll;
use futures::Stream;
use rand_core::{OsRng, RngCore};
use std::error::Error as StdError;
use std::pin::Pin;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncryptionStreamState {
    Start,
    Stream,
    Done,
    Error,
}

pub struct EncryptionStream<E: StdError, S: Stream<Item = Result<Bytes, E>>> {
    stream: Pin<Box<S>>,
    state: EncryptionStreamState,
    nonce: XNonce,
    crypt: XChaCha20,
}

impl<E: StdError, S: Stream<Item = Result<Bytes, E>>> EncryptionStream<E, S> {
    pub fn new(stream: S, key: &Key) -> Self {
        // generate nonce
        let mut nonce = [0u8; 24];
        OsRng.fill_bytes(&mut nonce);
        let nonce = XNonce::from_slice(&nonce);

        EncryptionStream {
            state: EncryptionStreamState::Start,
            stream: Box::pin(stream),
            nonce: nonce.clone(),
            crypt: XChaCha20::new(key, nonce),
        }
    }
}

impl<E: StdError, S: Stream<Item = Result<Bytes, E>>> Stream for EncryptionStream<E, S> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use EncryptionStreamState::*;
        match self.state.clone() {
            Start => {
                self.state = Stream;
                Poll::Ready(Some(Ok(Bytes::copy_from_slice(self.nonce.as_slice()))))
            }
            Stream => match Pin::new(&mut self.stream).poll_next(cx) {
                error @ Poll::Ready(Some(Err(_))) => {
                    self.state = Error;
                    error
                }
                done @ Poll::Ready(None) => {
                    self.state = Done;
                    done
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    let mut bytes = bytes.as_ref().to_vec();
                    self.crypt.apply_keystream(&mut bytes);
                    Poll::Ready(Some(Ok(Bytes::copy_from_slice(&bytes))))
                }
                _ => unimplemented!(),
            },
            Done | Error => Poll::Ready(None),
        }
    }
}

#[tokio::test]
async fn test_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![]);
    let mut crypt_stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);
}

#[tokio::test]
async fn test_one_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![Ok(Bytes::copy_from_slice(b"hello"))]);
    let mut crypt_stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 5);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);
}

#[tokio::test]
async fn test_multiple_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![
        Ok(Bytes::copy_from_slice(b"hello")),
        Ok(Bytes::copy_from_slice(b"there")),
        Ok(Bytes::copy_from_slice(b"world!")),
    ]);
    let mut crypt_stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 5);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 5);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 6);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);
}

#[tokio::test]
async fn test_error_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![
        Ok(Bytes::copy_from_slice(b"hello")),
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "error")),
        Ok(Bytes::copy_from_slice(b"world!")),
    ]);
    let mut crypt_stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap().len(), 5);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert!(result.is_err());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Error);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Error);
}
