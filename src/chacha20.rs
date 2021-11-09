use bytes::{Buf, BufMut, Bytes, BytesMut};
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

enum DecryptionStreamState {
    Start(Key, BytesMut),
    Stream(XChaCha20),
    Done,
    Error,
}

pub struct DecryptionStream<E: StdError, S: Stream<Item = Result<Bytes, E>>> {
    stream: Pin<Box<S>>,
    state: DecryptionStreamState,
}

impl<E: StdError, S: Stream<Item = Result<Bytes, E>>> DecryptionStream<E, S> {
    pub fn new(stream: S, key: &Key) -> Self {
        DecryptionStream {
            stream: Box::pin(stream),
            state: DecryptionStreamState::Start(key.clone(), BytesMut::with_capacity(24)),
        }
    }
}

impl<E: StdError, S: Stream<Item = Result<Bytes, E>>> Stream for DecryptionStream<E, S> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use DecryptionStreamState::*;
        let mut result = Pin::new(&mut self.stream).poll_next(cx);
        match &mut self.state {
            Start(key, nonce) => match result {
                Poll::Ready(Some(Ok(mut bytes))) => {
                    println!("nonce len is: {}", nonce.len());
                    let nonce_data = bytes.split_to((24 - nonce.len()).min(bytes.len()));
                    nonce.put(nonce_data);
                    if nonce.len() == 24 {
                        let nonce = XNonce::from_slice(&nonce);
                        let mut crypter = XChaCha20::new(&key, &nonce);
                        let mut bytes: BytesMut = bytes.chunk().into();
                        crypter.apply_keystream(&mut bytes);
                        self.state = DecryptionStreamState::Stream(crypter);
                        Poll::Ready(Some(Ok(bytes.freeze())))
                    } else {
                        Poll::Ready(Some(Ok(bytes)))
                    }
                }
                error @ Poll::Ready(Some(Err(_))) => {
                    self.state = DecryptionStreamState::Error;
                    error
                }
                done @ Poll::Ready(None) => {
                    self.state = DecryptionStreamState::Done;
                    done
                }
                result => result,
            },
            Stream(xchacha) => match result {
                Poll::Ready(Some(Ok(mut bytes))) => {
                    let mut bytes: BytesMut = bytes.chunk().into();
                    xchacha.apply_keystream(&mut bytes);
                    Poll::Ready(Some(Ok(bytes.freeze())))
                }
                error @ Poll::Ready(Some(Err(_))) => {
                    self.state = DecryptionStreamState::Error;
                    error
                }
                done @ Poll::Ready(None) => {
                    self.state = DecryptionStreamState::Done;
                    done
                }
                result => result,
            },
            Done | Error => Poll::Ready(None),
        }
    }
}

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
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

#[cfg(test)]
#[tokio::test]
async fn test_error_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![
        Ok(Bytes::copy_from_slice(b"hello")),
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "error",
        )),
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

#[cfg(test)]
#[tokio::test]
async fn test_decrypt_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let nonce: Bytes = "s91hd9v0-dk2ldlv;as920di".into();
    let stream = futures::stream::iter(vec![Ok(nonce)]);
    let mut crypt_stream = DecryptionStream::<std::io::Error, _>::new(stream, key);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());
    assert!(crypt_stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn test_decrypt_empty_stream2() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let nonce1: Bytes = "s91hd9v0-dk".into();
    let nonce2: Bytes = "2ldlv;as920di".into();
    let stream = futures::stream::iter(vec![Ok(nonce1), Ok(nonce2)]);
    let mut crypt_stream = DecryptionStream::<std::io::Error, _>::new(stream, key);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    assert!(crypt_stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn test_endtoend_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![]);
    let stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error, _>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn test_endtoend_single_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let data: Bytes = "hello, world!".into();
    let stream = futures::stream::iter(vec![Ok(data.clone())]);
    let stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error, _>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data);

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn test_endtoend_multi_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let data1: Bytes = "hello, world!".into();
    let data2: Bytes = "this is an example".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let stream = EncryptionStream::<std::io::Error, _>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error, _>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data1);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data2);

    assert!(stream.next().await.is_none());
}
