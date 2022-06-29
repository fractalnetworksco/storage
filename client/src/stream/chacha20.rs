use bytes::{Buf, BufMut, Bytes, BytesMut};
use chacha20::cipher::{NewCipher, StreamCipher};
use chacha20::{Key, XChaCha20, XNonce};
use futures::task::Context;
use futures::task::Poll;
use futures::Stream;
use log::debug;
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

pub struct EncryptionStream<E: StdError> {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send + Sync>>,
    state: EncryptionStreamState,
    nonce: XNonce,
    crypt: XChaCha20,
}

impl<E: StdError> EncryptionStream<E> {
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + Sync + 'static>(
        stream: S,
        key: &Key,
    ) -> Self {
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

impl<E: StdError> Stream for EncryptionStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use EncryptionStreamState::*;
        match self.state.clone() {
            Start => {
                let mut nonce: BytesMut = self.nonce.as_slice().into();
                self.state = Stream;
                match Pin::new(&mut self.stream).poll_next(cx) {
                    Poll::Ready(Some(Ok(bytes))) => {
                        let mut bytes = bytes.as_ref().to_vec();
                        self.crypt.apply_keystream(&mut bytes);
                        nonce.extend_from_slice(&bytes[..]);
                    }
                    Poll::Ready(Some(Err(error))) => {
                        self.state = Error;
                        return Poll::Ready(Some(Err(error)));
                    }
                    Poll::Ready(None) => {
                        self.state = Done;
                    }
                    Poll::Pending => {}
                }
                debug!("Sending nonce: {nonce:?}");
                Poll::Ready(Some(Ok(nonce.freeze())))
            }
            Stream => match Pin::new(&mut self.stream).poll_next(cx) {
                Poll::Ready(Some(Err(error))) => {
                    debug!("Read error from stream: {error:?}");
                    self.state = Error;
                    Poll::Ready(Some(Err(error)))
                }
                done @ Poll::Ready(None) => {
                    debug!("Stream closed");
                    self.state = Done;
                    done
                }
                Poll::Ready(Some(Ok(bytes))) => {
                    debug!("Read bytes: {bytes:?}");
                    let mut bytes = bytes.as_ref().to_vec();
                    self.crypt.apply_keystream(&mut bytes);
                    Poll::Ready(Some(Ok(Bytes::copy_from_slice(&bytes))))
                }
                other => other,
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

pub struct DecryptionStream<E: StdError> {
    stream: Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send>>,
    state: DecryptionStreamState,
}

impl<E: StdError> DecryptionStream<E> {
    pub fn new<S: Stream<Item = Result<Bytes, E>> + Send + 'static>(stream: S, key: &Key) -> Self {
        DecryptionStream {
            stream: Box::pin(stream),
            state: DecryptionStreamState::Start(key.clone(), BytesMut::with_capacity(24)),
        }
    }
}

impl<E: StdError> Stream for DecryptionStream<E> {
    type Item = Result<Bytes, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use DecryptionStreamState::*;
        let result = Pin::new(&mut self.stream).poll_next(cx);
        match &mut self.state {
            Start(key, nonce) => match result {
                Poll::Ready(Some(Ok(mut bytes))) => {
                    debug!("Read raw data: {bytes:?}");
                    let nonce_data = bytes.split_to((24 - nonce.len()).min(bytes.len()));
                    debug!("Putting nonce data: {nonce_data:?}");
                    nonce.put(nonce_data);
                    if nonce.len() == 24 {
                        debug!("Got nonce {nonce:?}");
                        let nonce = XNonce::from_slice(&nonce);
                        let mut crypter = XChaCha20::new(&key, &nonce);
                        let mut bytes: BytesMut = bytes.chunk().into();
                        debug!("Decrypting {bytes:?}");
                        crypter.apply_keystream(&mut bytes);
                        debug!("Decrypted {bytes:?}");
                        self.state = DecryptionStreamState::Stream(crypter);
                        Poll::Ready(Some(Ok(bytes.freeze())))
                    } else {
                        Poll::Ready(Some(Ok(Bytes::new())))
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
                Poll::Ready(Some(Ok(bytes))) => {
                    debug!("Read raw data: {bytes:?}");
                    let mut bytes: BytesMut = bytes.chunk().into();
                    debug!("Decrypting: {bytes:?}");
                    xchacha.apply_keystream(&mut bytes);
                    debug!("Decrypted: {bytes:?}");
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
async fn encrypt_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![]);
    let mut crypt_stream = EncryptionStream::<std::io::Error>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);
}

#[cfg(test)]
#[tokio::test]
async fn encrypt_single_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![Ok(Bytes::copy_from_slice(b"hello"))]);
    let mut crypt_stream = EncryptionStream::<std::io::Error>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let _result = crypt_stream.next().await.unwrap();
    //assert_eq!(result.unwrap(), crypt_stream.nonce.as_slice());
    //assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    //let result = crypt_stream.next().await.unwrap();
    //assert_eq!(result.unwrap().len(), 5);
    //assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Done);
}

#[cfg(test)]
#[tokio::test]
async fn encrypt_multi_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![
        Ok(Bytes::copy_from_slice(b"hello")),
        Ok(Bytes::copy_from_slice(b"there")),
        Ok(Bytes::copy_from_slice(b"world!")),
    ]);
    let mut crypt_stream = EncryptionStream::<std::io::Error>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(
        result.unwrap().len(),
        crypt_stream.nonce.as_slice().len() + 5
    );
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
async fn encrypt_error_stream() {
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
    let mut crypt_stream = EncryptionStream::<std::io::Error>::new(stream, key);
    assert_eq!(crypt_stream.state, EncryptionStreamState::Start);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(
        result.unwrap().len(),
        crypt_stream.nonce.as_slice().len() + 5
    );
    assert_eq!(crypt_stream.state, EncryptionStreamState::Stream);

    let result = crypt_stream.next().await.unwrap();
    assert!(result.is_err());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Error);

    assert!(crypt_stream.next().await.is_none());
    assert_eq!(crypt_stream.state, EncryptionStreamState::Error);
}

#[cfg(test)]
#[tokio::test]
async fn decrypt_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let nonce: Bytes = "s91hd9v0-dk2ldlv;as920di".into();
    let stream = futures::stream::iter(vec![Ok(nonce)]);
    let mut crypt_stream = DecryptionStream::<std::io::Error>::new(stream, key);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());
    assert!(crypt_stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn decrypt_empty_stream2() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let nonce1: Bytes = "s91hd9v0-dk".into();
    let nonce2: Bytes = "2ldlv;as920di".into();
    let stream = futures::stream::iter(vec![Ok(nonce1), Ok(nonce2)]);
    let mut crypt_stream = DecryptionStream::<std::io::Error>::new(stream, key);

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    let result = crypt_stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    assert!(crypt_stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn endtoend_empty_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let stream = futures::stream::iter(vec![]);
    let stream = EncryptionStream::<std::io::Error>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), Bytes::new());

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn endtoend_single_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let data: Bytes = "hello, world!".into();
    let stream = futures::stream::iter(vec![Ok(data.clone())]);
    let stream = EncryptionStream::<std::io::Error>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data);

    assert!(stream.next().await.is_none());
}

#[cfg(test)]
#[tokio::test]
async fn endtoend_multi_stream() {
    use futures::StreamExt;
    let key = Key::from_slice(b"abcdefghijklmnopqrstuvwxyz012345");
    let data1: Bytes = "hello, world!".into();
    let data2: Bytes = "this is an example".into();
    let stream = futures::stream::iter(vec![Ok(data1.clone()), Ok(data2.clone())]);
    let stream = EncryptionStream::<std::io::Error>::new(stream, key);
    let mut stream = DecryptionStream::<std::io::Error>::new(stream, key);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data1);

    let result = stream.next().await.unwrap();
    assert_eq!(result.unwrap(), data2);

    assert!(stream.next().await.is_none());
}
