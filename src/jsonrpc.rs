//! A subset of JSON-RPC types used by the Language Server Protocol.

mod error;
mod pending;

pub use self::error::{Error, ErrorCode};
pub(crate) use self::pending::{ClientRequests, ServerRequests};
use serde::{
    de::{self, Deserializer},
    ser::Serializer,
    Deserialize,
    Serialize,
};
use serde_json::Value;
use std::{
    borrow::Cow,
    fmt::{self, Debug, Display, Formatter},
};

/// A specialized [`Result`] error type for JSON-RPC handlers.
///
/// [`Result`]: enum@std::result::Result
pub type Result<T> = std::result::Result<T, Error>;

/// A unique ID used to correlate requests and responses together.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Id {
    /// Numeric ID.
    Number(u64),
    /// String ID.
    String(String),
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Id::Number(id) => Display::fmt(id, f),
            Id::String(id) => Debug::fmt(id, f),
        }
    }
}

/// A successful or failed JSON-RPC response.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct Response {
    jsonrpc: Version,
    #[serde(flatten)]
    kind: ResponseKind,
}

impl Response {
    /// Creates a new successful response from a request ID and `Error` object.
    pub const fn ok(id: Id, result: Value) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Ok { result, id },
        }
    }

    /// Creates a new error response from a request ID and `Error` object.
    pub const fn error(id: Option<Id>, error: Error) -> Self {
        Response {
            jsonrpc: Version,
            kind: ResponseKind::Err { error, id },
        }
    }

    /// Creates a new response from a request ID and either an `Ok(Value)` or `Err(Error)` body.
    pub fn from_parts(id: Id, body: Result<Value>) -> Self {
        match body {
            Ok(result) => Response::ok(id, result),
            Err(error) => Response::error(Some(id), error),
        }
    }

    /// Splits the response into a request ID paired with either an `Ok(Value)` or `Err(Error)` to
    /// signify whether the response is a success or failure.
    pub fn into_parts(self) -> (Option<Id>, Result<Value>) {
        match self.kind {
            ResponseKind::Ok { id, result } => (Some(id), Ok(result)),
            ResponseKind::Err { id, error } => (id, Err(error)),
        }
    }

    /// Returns the corresponding request ID, if any.
    pub fn id(&self) -> Option<&Id> {
        match self.kind {
            ResponseKind::Ok { ref id, .. } => Some(id),
            ResponseKind::Err { ref id, .. } => id.as_ref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
enum ResponseKind {
    Ok { result: Value, id: Id },
    Err { error: Error, id: Option<Id> },
}

#[allow(clippy::large_enum_variant)]
/// An incoming JSON-RPC message.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(test, derive(Serialize))]
#[serde(untagged)]
pub enum Incoming {
    /// Request intended for the language server.
    Request(Box<crate::generated_impl::ServerRequest>),
    /// Response to a server-to-client request.
    Response(Response),
}

/// A server-to-client LSP request.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
pub struct ClientRequest {
    jsonrpc: Version,
    method: Cow<'static, str>,
    #[serde(flatten)]
    kind: ClientMethod,
}

impl ClientRequest {
    /// Constructs a JSON-RPC request from its corresponding LSP type.
    pub(crate) fn request<R: lsp::request::Request>(id: u64, params: R::Params) -> Self {
        // Since `R::Params` come from the `lsp-types` crate and validity is enforced via the
        // `Request` trait, the `unwrap()` call below should never fail.
        ClientRequest {
            jsonrpc: Version,
            method: R::METHOD.into(),
            kind: ClientMethod::Request {
                params: serde_json::to_value(params).unwrap(),
                id: Id::Number(id),
            },
        }
    }

    /// Constructs a JSON-RPC notification from its corresponding LSP type.
    pub(crate) fn notification<N: lsp::notification::Notification>(params: N::Params) -> Self {
        // Since `N::Params` comes from the `lsp-types` crate and validity is enforced via the
        // `Notification` trait, the `unwrap()` call below should never fail.
        ClientRequest {
            jsonrpc: Version,
            method: N::METHOD.into(),
            kind: ClientMethod::Notification {
                params: serde_json::to_value(params).unwrap(),
            },
        }
    }
}

impl Display for ClientRequest {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut w = WriterFormatter { inner: f };
        serde_json::to_writer(&mut w, self).map_err(|_| fmt::Error)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(untagged)]
enum ClientMethod {
    Request { params: Value, id: Id },
    Notification { params: Value },
}

/// An outgoing JSON-RPC message.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[cfg_attr(test, derive(Deserialize))]
#[serde(untagged)]
pub enum Outgoing {
    /// Response to a client-to-server request.
    Response(Response),
    /// Request intended for the language client.
    Request(ClientRequest),
}

impl Display for Outgoing {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let mut w = WriterFormatter { inner: f };
        serde_json::to_writer(&mut w, self).map_err(|_| fmt::Error)
    }
}

struct WriterFormatter<'a, 'b: 'a> {
    inner: &'a mut Formatter<'b>,
}

impl<'a, 'b> std::io::Write for WriterFormatter<'a, 'b> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        fn io_error<E>(_: E) -> std::io::Error {
            // Error value does not matter because fmt::Display impl below just
            // maps it to fmt::Error
            std::io::Error::new(std::io::ErrorKind::Other, "fmt error")
        }
        let s = std::str::from_utf8(buf).map_err(io_error)?;
        self.inner.write_str(s).map_err(io_error)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Version;

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Cow::<'de, str>::deserialize(deserializer)?.as_ref() {
            "2.0" => Ok(Version),
            _ => Err(de::Error::custom("expected JSON-RPC version \"2.0\"")),
        }
    }
}

impl Serialize for Version {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        "2.0".serialize(serializer)
    }
}

/// Error response returned for every request received before the server is initialized.
///
/// See [here](https://microsoft.github.io/language-server-protocol/specification#initialize)
/// for reference.
pub(crate) fn not_initialized_error() -> Error {
    Error {
        code: ErrorCode::ServerError(-32002),
        message: "Server not initialized".to_string(),
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod client_request {
        use super::*;

        #[test]
        fn display() {
            let id = 0;
            let request = ClientRequest::request::<lsp::request::Shutdown>(id, ());
            format!("{}", request);
        }
    }

    mod id {
        use super::*;

        #[test]
        fn number_display() {
            let id = Id::Number(42);
            assert_eq!("42", format!("{}", id));
        }

        #[test]
        fn string_display() {
            let id = Id::String("foo".into());
            assert_eq!("\"foo\"", format!("{}", id));
        }
    }

    mod outgoing {
        use super::*;
        use serde_json::json;

        #[test]
        fn display_response() {
            let id = Id::Number(1);
            let response = Response::ok(id, json!({}));
            let outgoing = Outgoing::Response(response);
            let json = json!({"jsonrpc": "2.0", "result": {}, "id": 1});
            assert_eq!(json.to_string(), format!("{}", outgoing));
        }

        #[test]
        fn display_client_request() {
            let id = 1;
            let params = {
                let typ = lsp::MessageType::INFO;
                let message = Default::default();
                let actions = Default::default();
                lsp::ShowMessageRequestParams { typ, message, actions }
            };
            let outgoing = Outgoing::Request(ClientRequest::request::<lsp::request::ShowMessageRequest>(id, params));
            let json = json!({
                "jsonrpc": "2.0",
                "method": "window/showMessageRequest",
                "params": { "type": 3, "message": ""},
                "id": 1
            });
            assert_eq!(json.to_string(), format!("{}", outgoing));
        }
    }

    mod response {
        use super::*;
        use serde_json::json;

        #[test]
        fn from_parts_ok() {
            let id = Id::Number(1);
            let value = Value::Null;
            let body = Ok(value.clone());
            let response = Response::from_parts(id.clone(), body.clone());
            assert_eq!(response, Response::ok(id.clone(), value));
            assert_eq!(response.into_parts(), (Some(id), body));
        }

        #[test]
        fn from_parts_err() {
            let id = Id::Number(1);
            let error = Error::internal_error();
            let body = Err(error.clone());
            let response = Response::from_parts(id.clone(), body.clone());
            assert_eq!(response, Response::error(Some(id.clone()), error));
            assert_eq!(response.into_parts(), (Some(id), body));
        }

        #[test]
        fn id_ok() {
            let id = Id::Number(1);
            let response = Response::ok(id.clone(), json!({}));
            assert_eq!(response.id(), Some(&id));
        }

        #[test]
        fn id_err() {
            let id = Id::Number(1);
            let response = Response::error(Some(id.clone()), Error::internal_error());
            assert_eq!(response.id(), Some(&id));
            let response = Response::error(None, Error::internal_error());
            assert_eq!(response.id(), None);
        }
    }

    mod version {
        use super::*;

        #[test]
        fn deserialize_valid() {
            let result = serde_json::from_str::<Version>("\"2.0\"");
            assert!(result.is_ok())
        }

        #[test]
        fn deserialize_invalid() {
            let result = serde_json::from_str::<Version>("\"1.0\"");
            assert!(result.is_err());
        }
    }
}
