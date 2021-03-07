//! Service abstraction for language servers.

use futures::{
    channel::mpsc,
    future,
    stream::{FusedStream, Stream},
    FutureExt,
};
use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower_service::Service;

/// Error that occurs when attempting to call the language server after it has already exited.
#[derive(Clone, Debug, PartialEq)]
pub struct ExitedError;

impl Display for ExitedError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str("language server has exited")
    }
}

impl Error for ExitedError {
}

/// Stream of messages produced by the language server.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct MessageStream(mpsc::Receiver<crate::jsonrpc::Outgoing>);

impl Stream for MessageStream {
    type Item = crate::jsonrpc::Outgoing;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
        let recv = &mut self.as_mut().0;
        Pin::new(recv).poll_next(cx)
    }
}

impl FusedStream for MessageStream {
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}

/// Service abstraction for the Language Server Protocol.
///
/// This service takes an incoming JSON-RPC message as input and produces an outgoing message as
/// output. If the incoming message is a server notification or a client response, then the
/// corresponding response will be `None`.
///
/// This implements [`tower_service::Service`] in order to remain independent from the underlying
/// transport and to facilitate further abstraction with middleware.
///
/// [`tower_service::Service`]: https://docs.rs/tower-service/0.3.0/tower_service/trait.Service.html
///
/// Pending requests can be canceled by issuing a [`$/cancelRequest`] notification.
///
/// [`$/cancelRequest`]: https://microsoft.github.io/language-server-protocol/specification#cancelRequest
///
/// The service shuts down and stops serving requests after the [`exit`] notification is received.
/// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
pub struct LspService {
    server: Arc<dyn crate::LanguageServer>,
    pending_server: crate::jsonrpc::ServerRequests,
    pending_client: Arc<crate::jsonrpc::ClientRequests>,
    state: Arc<crate::server::State>,
}

impl LspService {
    /// Creates a new `LspService` with the given server backend, also returning a stream of
    /// notifications from the server back to the client.
    pub fn new<T, F>(init: F) -> (Self, MessageStream)
    where
        F: FnOnce(crate::client::Client) -> T,
        T: crate::LanguageServer,
    {
        let state = Arc::new(crate::server::State::new());
        let (tx, rx) = mpsc::channel(1);
        let messages = MessageStream(rx);

        let pending_client = Arc::new(crate::jsonrpc::ClientRequests::new());
        let client = crate::client::Client::new(tx, pending_client.clone(), state.clone());

        let service = LspService {
            server: Arc::from(init(client)),
            pending_server: crate::jsonrpc::ServerRequests::new(),
            pending_client,
            state,
        };

        (service, messages)
    }
}

impl Service<crate::jsonrpc::Incoming> for LspService {
    type Error = ExitedError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
    type Response = Option<crate::jsonrpc::Outgoing>;

    fn poll_ready(&mut self, _: &mut Context) -> Poll<Result<(), Self::Error>> {
        if self.state.get() == crate::server::StateKind::Exited {
            Poll::Ready(Err(ExitedError))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn call(&mut self, request: crate::jsonrpc::Incoming) -> Self::Future {
        if self.state.get() == crate::server::StateKind::Exited {
            future::err(ExitedError).boxed()
        } else {
            match request {
                crate::jsonrpc::Incoming::Request(req) => {
                    super::generated_impl::handle_request(self.server.clone(), &self.state, &self.pending_server, req)
                },
                crate::jsonrpc::Incoming::Response(res) => {
                    log::trace!("received client response: {:?}", res);
                    self.pending_client.insert(res);
                    future::ok(None).boxed()
                },
            }
        }
    }
}

impl Debug for LspService {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct(stringify!(LspService))
            .field("pending_server", &self.pending_server)
            .field("pending_client", &self.pending_client)
            .field("state", &self.state)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use tower_test::mock::Spawn;

    const INITIALIZE_REQUEST: &str =
        r#"{ "jsonrpc": "2.0", "method": "initialize", "params": { "capabilities": {} }, "id": 1 }"#;
    const INITIALIZED_NOTIF: &str = r#"{ "jsonrpc": "2.0", "method": "initialized", "params": {} }"#;
    const SHUTDOWN_REQUEST: &str = r#"{ "jsonrpc": "2.0", "method": "shutdown", "id": 1 }"#;
    const EXIT_NOTIF: &str = r#"{ "jsonrpc": "2.0", "method": "exit" }"#;

    #[derive(Debug, Default)]
    struct Mock;

    #[async_trait]
    impl crate::LanguageServer for Mock {
        async fn initialize(&self, _: lsp::InitializeParams) -> crate::jsonrpc::Result<lsp::InitializeResult> {
            Ok(lsp::InitializeResult::default())
        }

        async fn shutdown(&self) -> crate::jsonrpc::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn call_response() {
        use crate::jsonrpc::{Id, Incoming, Response};

        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialize: crate::jsonrpc::Incoming = serde_json::from_str(INITIALIZE_REQUEST).unwrap();
        let raw = json!({ "jsonrpc": "2.0", "result": { "capabilities": {} }, "id": 1 });
        let ok = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize.clone()).await, Ok(Some(ok)));

        let incoming = Response::ok(Id::Number(0), json!(null));
        assert_eq!(service.call(Incoming::Response(incoming)).await, Ok(None));
    }

    #[test]
    fn debug() {
        let (service, _) = LspService::new(|_| Mock::default());
        format!("{:?}", service);
    }

    #[tokio::test]
    async fn initializes_only_once() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialize: crate::jsonrpc::Incoming = serde_json::from_str(INITIALIZE_REQUEST).unwrap();
        let raw = json!({ "jsonrpc": "2.0", "result": { "capabilities": {} }, "id": 1 });
        let ok = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize.clone()).await, Ok(Some(ok)));

        let raw = json!({ "jsonrpc": "2.0", "error": { "code": -32600, "message": "Invalid request" }, "id": 1 });
        let err = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize).await, Ok(Some(err)));
    }

    #[tokio::test]
    async fn refuses_requests_after_shutdown() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialize: crate::jsonrpc::Incoming = serde_json::from_str(INITIALIZE_REQUEST).unwrap();
        let raw = json!({ "jsonrpc": "2.0", "result": { "capabilities": {} }, "id":1 });
        let ok = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialize.clone()).await, Ok(Some(ok)));

        let shutdown: crate::jsonrpc::Incoming = serde_json::from_str(SHUTDOWN_REQUEST).unwrap();
        let raw = json!({ "jsonrpc": "2.0", "result": null, "id": 1 });
        let ok = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(shutdown.clone()).await, Ok(Some(ok)));

        let raw = json!({ "jsonrpc": "2.0", "error": { "code": -32600, "message": "Invalid request" }, "id": 1 });
        let err = serde_json::from_value(raw).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(shutdown).await, Ok(Some(err)));
    }

    #[tokio::test]
    async fn exit_notification() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        let initialized: crate::jsonrpc::Incoming = serde_json::from_str(INITIALIZED_NOTIF).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(initialized.clone()).await, Ok(None));

        let exit: crate::jsonrpc::Incoming = serde_json::from_str(EXIT_NOTIF).unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(exit).await, Ok(None));

        assert_eq!(service.poll_ready(), Poll::Ready(Err(ExitedError)));
        assert_eq!(service.call(initialized).await, Err(ExitedError));
    }

    mod exited_error {
        use super::*;

        #[test]
        fn display() {
            let error = ExitedError;
            let display = format!("{}", error);
            assert_eq!("language server has exited", display);
        }
    }

    mod message_stream {
        use super::*;
        use futures::StreamExt;

        #[tokio::test]
        async fn is_terminated() {
            let (_, mut messages) = LspService::new(|_| Mock::default());
            assert_eq!(messages.is_terminated(), false);
            while messages.next().await.is_some() {}
            assert_eq!(messages.is_terminated(), true);
        }

        #[tokio::test]
        async fn poll_next() {
            let (_, mut messages) = LspService::new(|_| Mock::default());
            messages.next().await;
        }
    }
}
