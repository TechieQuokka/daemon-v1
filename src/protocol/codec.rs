use serde::{de::DeserializeOwned, Serialize};
use std::io;
use tokio_util::codec::{Decoder, Encoder, LinesCodec};

/// JSON line codec for stdin/stdout communication
pub struct JsonLineCodec<T> {
    inner: LinesCodec,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> Default for JsonLineCodec<T> {
    fn default() -> Self {
        Self {
            inner: LinesCodec::new(),
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<T> JsonLineCodec<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: DeserializeOwned> Decoder for JsonLineCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match self.inner.decode(src).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, e.to_string())
        })? {
            Some(line) => {
                let item = serde_json::from_str(&line).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
                })?;
                Ok(Some(item))
            }
            None => Ok(None),
        }
    }
}

impl<T: Serialize> Encoder<T> for JsonLineCodec<T> {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut bytes::BytesMut) -> Result<(), Self::Error> {
        let json = serde_json::to_string(&item)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        self.inner.encode(json, dst)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestMessage {
        cmd: String,
        value: i32,
    }

    #[test]
    fn test_encode_decode() {
        let mut codec: JsonLineCodec<TestMessage> = JsonLineCodec::new();
        let msg = TestMessage {
            cmd: "test".to_string(),
            value: 42,
        };

        let mut buf = BytesMut::new();
        codec.encode(msg.clone(), &mut buf).unwrap();

        let decoded = codec.decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded, msg);
    }
}
