#[cfg(feature = "runtime-agnostic")]
use async_codec_lite::{Decoder, Encoder};
#[cfg(feature = "runtime-tokio")]
use tokio_util::codec::{Decoder, Encoder};

use bytes::{Buf, BufMut, BytesMut};

use std::{
    io::{self, Write},
    marker::PhantomData,
};
use thiserror::Error;

/// Errors that can occur when processing an LSP request.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Failed to parse the JSON body.
    #[error("failed to parse JSON body: {0}")]
    Body(serde_json::Error),
    /// Failed to encode the response.
    #[error("failed to encode response: {0}")]
    Encode(io::Error),
    /// Failed to parse headers.
    #[error("failed to parse headers: {0}")]
    Httparse(httparse::Error),
    /// The length value in the `Content-Length` header is invalid.
    #[error("invalid content length value")]
    InvalidLength,
    /// Request lacks the required `Content-Length` header.
    #[error("missing required `Content-Length` header")]
    MissingHeader,
    /// Request contains invalid UTF8.
    #[error("request contains invalid UTF-8: {0}")]
    Utf8(std::str::Utf8Error),
}

impl From<io::Error> for ParseError {
    fn from(error: io::Error) -> Self {
        ParseError::Encode(error)
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(error: serde_json::Error) -> Self {
        ParseError::Body(error)
    }
}

impl From<std::str::Utf8Error> for ParseError {
    fn from(error: std::str::Utf8Error) -> Self {
        ParseError::Utf8(error)
    }
}

/// Encodes and decodes Language Server Protocol messages.
#[derive(Clone, Debug)]
pub struct LanguageServerCodec<T> {
    remaining_msg_bytes: usize,
    _marker: PhantomData<T>,
}

impl<T> Default for LanguageServerCodec<T> {
    fn default() -> Self {
        LanguageServerCodec {
            remaining_msg_bytes: 0,
            _marker: PhantomData,
        }
    }
}

#[cfg(feature = "runtime-agnostic")]
impl<T: serde::Serialize> Encoder for LanguageServerCodec<T> {
    type Error = ParseError;
    type Item = T;

    fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = serde_json::to_string(&item)?;
        log::trace!("-> {}", msg);

        // Reserve just enough space to hold the `Content-Length: ` and `\r\n\r\n` constants,
        // the length of the message, and the message body.
        dst.reserve(msg.len() + number_of_digits(msg.len()) + 20);
        let mut writer = dst.writer();
        write!(writer, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
        writer.flush()?;

        Ok(())
    }
}

#[cfg(feature = "runtime-tokio")]
impl<T: serde::Serialize> Encoder<T> for LanguageServerCodec<T> {
    type Error = ParseError;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let msg = serde_json::to_string(&item)?;
        log::trace!("-> {}", msg);

        // Reserve just enough space to hold the `Content-Length: ` and `\r\n\r\n` constants,
        // the length of the message, and the message body.
        dst.reserve(msg.len() + number_of_digits(msg.len()) + 20);
        let mut writer = dst.writer();
        write!(writer, "Content-Length: {}\r\n\r\n{}", msg.len(), msg)?;
        writer.flush()?;

        Ok(())
    }
}

#[inline]
fn number_of_digits(mut n: usize) -> usize {
    let mut num_digits = 0;

    while n > 0 {
        n /= 10;
        num_digits += 1;
    }

    num_digits
}

impl<T: serde::de::DeserializeOwned> Decoder for LanguageServerCodec<T> {
    type Error = ParseError;
    type Item = T;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if self.remaining_msg_bytes > src.len() {
            return Ok(None);
        }

        // The potential error returned by httparse
        let mut http_error = None;
        // The value of the "Content-Length" header
        let mut content_len = None;
        // The total length of the parsed http headers
        let mut headers_len = None;

        {
            // Placeholder used for parsing headers into
            let dst = &mut [httparse::EMPTY_HEADER; 2];

            // Parse the headers and try to extract values for the previous mut vars
            match httparse::parse_headers(src, dst) {
                // A complete set of headers was parsed succesfully
                Ok(httparse::Status::Complete((offset, headers))) => {
                    // If some headers were parsed successefully, set the headers length
                    headers_len = Some(offset);
                    // Scan through the headers
                    for header in headers {
                        // If the "Content-Length" header is found, parse the value as a usize
                        if header.name == "Content-Length" {
                            let value = std::str::from_utf8(header.value)?;
                            let value = value.parse::<usize>().map_err(|_| ParseError::InvalidLength)?;
                            let delta = offset + value;
                            // Ensure that the source bytes is long enough for us to decode the full content ...
                            if src.len() < delta {
                                // ... otherwise set the remaining num of bytes needed (avoids unnecessary reparsing)
                                self.remaining_msg_bytes = delta - src.len();
                                // ... then return None and wait for more input
                                return Ok(None);
                            } else {
                                content_len = Some(value);
                            }
                        }
                    }
                },
                // No errors occurred during parsing yet but no complete set of headers were parsed
                Ok(httparse::Status::Partial) => {
                    // Return None and wait for more input
                    return Ok(None);
                },
                // An error occurred during parsing of the headers
                Err(error) => {
                    http_error = Some(error);
                },
            }
        }

        // Headers were parsed (but "Content-Length" wasn't found) or an error occurred during parsing
        if content_len.is_none() || http_error.is_some() {
            // Maybe there are garbage prefix bytes so try to scan ahead for "Content-Length" to recover
            if let Some(offset) = twoway::find_bytes(src, b"Content-Length") {
                src.advance(offset);
            }
            // Then handle the conditions that caused decoding to fail ...
            if let Some(http_error) = http_error {
                // ... there was an error parsing the headers
                return Err(ParseError::Httparse(http_error));
            } else {
                // ... there was no "Content-Length" header found
                return Err(ParseError::MissingHeader);
            }
        }

        // The total length of the headers and the content
        let delta;

        // The result of parsing the content
        let result = if let (Some(headers_len), Some(content_len)) = (headers_len, content_len) {
            delta = headers_len + content_len;
            let msg = &src[headers_len .. delta];
            let msg = std::str::from_utf8(msg)?;
            log::trace!("<- {}", msg);
            match serde_json::from_str(msg) {
                Ok(parsed) => Ok(Some(parsed)),
                Err(err) => Err(err.into()),
            }
        } else {
            unreachable!()
        };

        src.advance(delta);
        self.remaining_msg_bytes = 0;

        result
    }
}

#[cfg(test)]
mod tests {
    use bytes::BytesMut;
    use serde_json::Value;

    use super::*;

    #[test]
    fn encode_and_decode() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::new();
        let item: Value = serde_json::from_str(&decoded).unwrap();
        codec.encode(item, &mut buffer).unwrap();
        assert_eq!(buffer, BytesMut::from(encoded.as_str()));

        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn decodes_optional_content_type() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let content_len = format!("Content-Length: {}", decoded.len());
        let content_type =
            "Content-Type: application/vscode-jsonrpc; charset=utf-8; foo=\"bar\\nbaz\\\"qux\\\"\"".to_string();
        let encoded = format!("{}\r\n{}\r\n\r\n{}", content_len, content_type, decoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(encoded.as_str());
        let message = codec.decode(&mut buffer).unwrap();
        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }

    #[test]
    fn recovers_from_parse_error() {
        let decoded = r#"{"jsonrpc":"2.0","method":"exit"}"#.to_string();
        let encoded = format!("Content-Length: {}\r\n\r\n{}", decoded.len(), decoded);
        let mixed = format!("1234567890abcdefgh{}", encoded);

        let mut codec = LanguageServerCodec::default();
        let mut buffer = BytesMut::from(mixed.as_str());

        match codec.decode(&mut buffer) {
            Err(ParseError::MissingHeader) => {},
            other => panic!("expected `Err(ParseError::MissingHeader)`, got {:?}", other),
        }

        let message = codec.decode(&mut buffer).unwrap();
        let decoded: Value = serde_json::from_str(&decoded).unwrap();
        assert_eq!(message, Some(decoded));
    }
}
