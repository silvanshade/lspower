#[cfg(feature = "runtime-agnostic")]
use async_codec_lite::{Decoder, Encoder};
#[cfg(feature = "runtime-tokio")]
use tokio_util::codec::{Decoder, Encoder};

use bytes::{Buf, BufMut, BytesMut};
use nom::{
    branch::alt,
    bytes::streaming::{is_not, tag, take},
    character::streaming::{crlf, digit1, space0},
    combinator::{map_res, opt},
};
use std::{
    io::{self, Write},
    marker::PhantomData,
    str,
};
use thiserror::Error;

/// Errors that can occur when processing an LSP request.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Request lacks the required `Content-Length` header.
    #[error("missing required `Content-Length` header")]
    MissingHeader,
    /// The length value in the `Content-Length` header is invalid.
    #[error("unable to parse content length")]
    InvalidLength,
    /// The media type in the `Content-Type` header is invalid.
    #[error("unable to parse content length")]
    InvalidType,
    /// Failed to parse the JSON body.
    #[error("unable to parse JSON body: {0}")]
    Body(serde_json::Error),
    /// Failed to encode the response.
    #[error("failed to encode response: {0}")]
    Encode(io::Error),
    /// Request contains invalid UTF8.
    #[error("request contains invalid UTF8: {0}")]
    Utf8(str::Utf8Error),
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

impl From<str::Utf8Error> for ParseError {
    fn from(error: str::Utf8Error) -> Self {
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

        let (msg, len) = match parse_message(src) {
            Ok((remaining, msg)) => (str::from_utf8(msg), src.len() - remaining.len()),
            Err(nom::Err::Incomplete(nom::Needed::Size(min))) => {
                self.remaining_msg_bytes = min.get();
                return Ok(None);
            },
            Err(nom::Err::Incomplete(_)) => {
                return Ok(None);
            },
            #[rustfmt::skip]
            | Err(nom::Err::Error  (nom::error::Error { code, .. }))
            | Err(nom::Err::Failure(nom::error::Error { code, .. })) => loop {
                use ParseError::*;
                match parse_message(src) {
                    Err(_) if !src.is_empty() => src.advance(1),
                    _ => match code {
                        nom::error::ErrorKind::Digit => return Err(InvalidLength),
                        nom::error::ErrorKind::MapRes => return Err(InvalidLength),
                        nom::error::ErrorKind::Char => return Err(InvalidType),
                        nom::error::ErrorKind::IsNot => return Err(InvalidType),
                        _ => return Err(MissingHeader),
                    },
                }
            },
        };

        let result = match msg {
            Err(err) => Err(err.into()),
            Ok(msg) => {
                log::trace!("<- {}", msg);
                match serde_json::from_str(msg) {
                    Ok(parsed) => Ok(Some(parsed)),
                    Err(err) => Err(err.into()),
                }
            },
        };

        src.advance(len);
        self.remaining_msg_bytes = 0;

        result
    }
}

fn parse_message(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
    let i = input;

    let (i, content_length) = (|input| {
        let i = input;
        let (i, _) = tag("Content-Length")(i)?;
        let (i, _) = space0(i)?;
        let (i, _) = tag(":")(i)?;
        let (i, _) = space0(i)?;
        let (i, content_length) = map_res(digit1, |s| -> Result<_, Box<dyn std::error::Error>> {
            str::from_utf8(s)?.parse::<usize>().map_err(Into::into)
        })(i)?;
        let (i, _) = crlf(i)?;
        Ok((i, content_length))
    })(i)?;

    let (i, _) = opt(|input| {
        let i = input;
        let (i, _) = tag("Content-Type")(i)?;
        let (i, _) = space0(i)?;
        let (i, _) = tag(":")(i)?;
        let (i, _) = is_not(";\r")(i)?;
        let (i, _) = opt(|input| {
            let i = input;
            let (i, _) = tag(";")(i)?;
            let (i, _) = space0(i)?;
            let (i, _) = tag("charset")(i)?;
            let (i, _) = space0(i)?;
            let (i, _) = tag("=")(i)?;
            let (i, _) = space0(i)?;
            let (i, charset) = opt(alt((tag("utf-8"), tag("utf8"))))(i)?;
            Ok((i, charset))
        })(i)?;
        let (i, _) = crlf(i)?;
        Ok((i, ()))
    })(i)?;

    let (i, _) = crlf(i)?;

    take(content_length)(i)
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
        let content_type = "Content-Type: application/vscode-jsonrpc; charset=utf-8".to_string();
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
