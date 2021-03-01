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

        let (msg, len) = match parse::message(src) {
            Ok((remaining, msg)) => (std::str::from_utf8(msg), src.len() - remaining.len()),
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
                match parse::message(src) {
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

mod parse {
    #![allow(unused)]

    use nom::{
        branch::alt,
        bytes::streaming::{escaped, tag, take},
        character::streaming::{char, crlf, digit1, multispace1, none_of, satisfy, space0},
        combinator::{map_res, not, opt, recognize},
        multi::{many0, many1},
        sequence::tuple,
    };

    struct ContentLength(usize);

    struct ContentType<'a> {
        mime_type: MimeType<'a>,
        parameters: Vec<Parameter<'a>>,
    }

    #[allow(unused)]
    struct MimeType<'a> {
        kind: &'a str,
        subkind: &'a str,
    }

    impl<'a> std::fmt::Display for MimeType<'a> {
        fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(fmt, "{}/{}", self.kind, self.subkind)
        }
    }

    #[allow(unused)]
    struct Parameter<'a> {
        attribute: &'a str,
        value: &'a str,
    }

    const TOKEN_SPECIALS: &[char] = &[
        '(', ')', '<', '>', '@', ',', ';', ':', '\\', '"', '/', '[', ']', '?', '.', '=',
    ];

    #[inline]
    fn content_length(input: &[u8]) -> nom::IResult<&[u8], ContentLength> {
        let i = input;
        let (i, _) = tag("Content-Length")(i)?;
        let (i, _) = space0(i)?;
        let (i, _) = tag(":")(i)?;
        let (i, _) = space0(i)?;
        let (i, length) = map_res(digit1, |s| -> Result<_, Box<dyn std::error::Error>> {
            std::str::from_utf8(s)?.parse::<usize>().map_err(Into::into)
        })(i)?;
        let (i, _) = crlf(i)?;
        Ok((i, ContentLength(length)))
    }

    #[inline]
    fn content_type(input: &[u8]) -> nom::IResult<&[u8], ContentType> {
        let i = input;
        let (i, _) = tag("Content-Type")(i)?;
        let (i, _) = space0(i)?;
        let (i, _) = tag(":")(i)?;
        let (i, _) = space0(i)?;
        let (i, mime_type) = content_type_mime(i)?;
        let (i, parameters) = many0(content_type_parameter)(i)?;
        let (i, _) = crlf(i)?;
        Ok((i, ContentType { mime_type, parameters }))
    }

    #[inline]
    fn content_type_mime(input: &[u8]) -> nom::IResult<&[u8], MimeType> {
        let i = input;
        let (i, kind) = map_res(
            recognize(many1(satisfy(|c| !c.is_whitespace() && c != '/'))),
            std::str::from_utf8,
        )(i)?;
        let (i, _) = tag("/")(i)?;
        let (i, subkind) = map_res(
            recognize(many1(satisfy(|c| !c.is_whitespace() && c != ';'))),
            std::str::from_utf8,
        )(i)?;
        Ok((i, MimeType { kind, subkind }))
    }

    #[inline]
    fn content_type_parameter(input: &[u8]) -> nom::IResult<&[u8], Parameter> {
        let i = input;
        let (i, _) = tuple((space0, tag(";"), space0))(i)?;
        let (i, attribute) = map_res(content_type_token, std::str::from_utf8)(i)?;
        let (i, _) = tuple((space0, tag("="), space0))(i)?;
        let (i, value) = map_res(
            alt((content_type_token, content_type_quoted_string)),
            std::str::from_utf8,
        )(i)?;
        Ok((i, Parameter { attribute, value }))
    }

    #[inline]
    fn content_type_quoted_string(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
        recognize(tuple((
            char('"'),
            escaped(none_of("\\\""), '\\', satisfy(|c| c.is_ascii())),
            char('"'),
        )))(input)
    }

    #[inline]
    fn content_type_token(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
        let i = input;
        let (i, token) = recognize(many1(satisfy(|c| {
            !c.is_control() && !c.is_whitespace() && !TOKEN_SPECIALS.contains(&c)
        })))(i)?;
        Ok((i, token))
    }

    #[inline]
    fn content_type_validate(content_type: &Option<ContentType>) {
        if let Some(ContentType { mime_type, parameters }) = content_type {
            if mime_type.kind != "application" || mime_type.subkind != "vscode-jsonrpc" {
                log::warn!(
                    "Expected MIME type: \"application/vscode-jsonrpc\"; Actual MIME type: \"{}\"",
                    mime_type
                );
            }
            if let Some(parameter) = parameters.iter().find(|p| p.attribute == "charset") {
                if !["utf-8", "utf8"].contains(&parameter.value) {
                    log::warn!(
                        "Expected \"charset\" value: \"utf-8\"; Actual \"charset\" value: \"{}\"",
                        parameter.value
                    );
                }
            }
        }
    }

    pub fn message(input: &[u8]) -> nom::IResult<&[u8], &[u8]> {
        let i = input;
        let (i, content_length) = content_length(i)?;
        let (i, content_type) = opt(content_type)(i)?;
        let (i, _) = crlf(i)?;
        #[cfg(debug_assertions)]
        content_type_validate(&content_type);
        take(content_length.0)(i)
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
