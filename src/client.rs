//! Types for sending data to and from the language client.

use futures::{
    channel::{mpsc, oneshot},
    future,
    select,
    sink::SinkExt,
    FutureExt,
};
use std::{
    convert::TryFrom,
    fmt::{self, Debug, Formatter},
    future::Future,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
};

/// A structure used to construct and cancel [`CancellationToken`].
pub struct TokenCanceller {
    cancelled: Arc<AtomicBool>,
    future: future::Shared<Pin<Box<dyn Future<Output = Result<(), oneshot::Canceled>> + Send>>>,
    sender: Option<oneshot::Sender<()>>,
}

impl TokenCanceller {
    /// Creates a new [`TokenCanceller`].
    pub fn new() -> Self {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = oneshot::channel();
        let future = receiver.boxed().shared();
        TokenCanceller {
            cancelled,
            future,
            sender: Some(sender),
        }
    }

    /// Signals to all subordinate [`CancellationToken`] that they have been cancelled.
    pub fn cancel(&mut self) {
        if let Some(sender) = self.sender.take() {
            if let Ok(()) = sender.send(()) {
                self.cancelled.store(true, Ordering::SeqCst);
            } else {
                unreachable!()
            }
        }
    }

    /// Create a subordinate [`CancellationToken`].
    pub fn token(&self) -> CancellationToken {
        let cancelled = self.cancelled.clone();
        let future = self.future.clone();
        CancellationToken { cancelled, future }
    }
}

impl Debug for TokenCanceller {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct(stringify!(LspService))
            .field("cancelled", &self.cancelled)
            .field("sender", &self.sender)
            .finish()
    }
}

impl Default for TokenCanceller {
    fn default() -> Self {
        Self::new()
    }
}

/// A token which listens for a cancellation signal from a [`TokenCanceller`].
#[derive(Debug, Clone)]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
    future: future::Shared<Pin<Box<dyn Future<Output = Result<(), oneshot::Canceled>> + Send>>>,
}

impl CancellationToken {
    /// Checks whether the cancellation signal has been given.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Returns a future which only resolves once the cancellation signal has been given.
    pub fn wait(&self) -> impl Future<Output = anyhow::Result<()>> {
        let future = self.future.clone();
        async move {
            future.await.map_err(anyhow::Error::msg)?;
            Ok(())
        }
    }
}

struct ClientInner {
    sender: mpsc::Sender<crate::jsonrpc::Outgoing>,
    request_id: AtomicU64,
    pending_requests: Arc<crate::jsonrpc::ClientRequests>,
    state: Arc<crate::server::State>,
}

/// Handle for communicating with the language client.
///
/// This type provides a very cheap implementation of [`Clone`] so API consumers can cheaply clone
/// and pass it around as needed.
///
/// [`Clone`]: trait@std::clone::Clone
#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

impl Client {
    pub(super) fn new(
        sender: mpsc::Sender<crate::jsonrpc::Outgoing>,
        pending_requests: Arc<crate::jsonrpc::ClientRequests>,
        state: Arc<crate::server::State>,
    ) -> Self {
        Client {
            inner: Arc::new(ClientInner {
                sender,
                request_id: AtomicU64::new(0),
                pending_requests,
                state,
            }),
        }
    }

    /// Notifies the client to log a particular message.
    ///
    /// This corresponds to the [`window/logMessage`] notification.
    ///
    /// [`window/logMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_logMessage
    pub async fn log_message<M: std::fmt::Display>(&self, typ: lsp::MessageType, message: M) {
        let message = message.to_string();
        let params = lsp::LogMessageParams { typ, message };
        self.send_notification::<lsp::notification::LogMessage>(params).await;
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub async fn show_message<M: std::fmt::Display>(&self, typ: lsp::MessageType, message: M) {
        let message = message.to_string();
        let params = lsp::ShowMessageParams { typ, message };
        self.send_notification::<lsp::notification::ShowMessage>(params).await;
    }

    /// Requests the client to display a particular message in the user interface.
    ///
    /// Unlike the `show_message` notification, this request can also pass a list of actions and
    /// wait for an answer from the client.
    ///
    /// This corresponds to the [`window/showMessageRequest`] request.
    ///
    /// [`window/showMessageRequest`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessageRequest
    #[rustfmt::skip]
    pub async fn show_message_request<M: std::fmt::Display>(
        &self,
        typ: lsp::MessageType,
        message: M,
        actions: Option<Vec<lsp::MessageActionItem>>,
    ) -> crate::jsonrpc::Result<Option<lsp::MessageActionItem>> {
        let token = None;
        let message = message.to_string();
        let params = lsp::ShowMessageRequestParams { typ, message, actions };
        self.send_request::<lsp::request::ShowMessageRequest>(params, token).await
    }

    /// Notifies the client to log a telemetry event.
    ///
    /// This corresponds to the [`telemetry/event`] notification.
    ///
    /// [`telemetry/event`]: https://microsoft.github.io/language-server-protocol/specification#telemetry_event
    pub async fn telemetry_event<S: serde::Serialize>(&self, data: S) {
        match serde_json::to_value(data) {
            Err(e) => log::error!("invalid JSON in `telemetry/event` notification: {}", e),
            Ok(mut value) => {
                if !value.is_null() && !value.is_array() && !value.is_object() {
                    value = serde_json::Value::Array(vec![value]);
                }
                self.send_notification::<lsp::notification::TelemetryEvent>(value).await;
            },
        }
    }

    /// Registers a new capability with the client.
    ///
    /// This corresponds to the [`client/registerCapability`] request.
    ///
    /// [`client/registerCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_registerCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    #[rustfmt::skip]
    pub async fn register_capability(&self, registrations: Vec<lsp::Registration>) -> crate::jsonrpc::Result<()> {
        let token = None;
        let params = lsp::RegistrationParams { registrations };
        self.send_request_initialized::<lsp::request::RegisterCapability>(params, token).await
    }

    /// Unregisters a capability with the client.
    ///
    /// This corresponds to the [`client/unregisterCapability`] request.
    ///
    /// [`client/unregisterCapability`]: https://microsoft.github.io/language-server-protocol/specification#client_unregisterCapability
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    #[rustfmt::skip]
    pub async fn unregister_capability(
        &self,
        unregisterations: Vec<lsp::Unregistration>,
    ) -> crate::jsonrpc::Result<()> {
        let token = None;
        let params = lsp::UnregistrationParams { unregisterations };
        self.send_request_initialized::<lsp::request::UnregisterCapability>(params, token).await
    }

    /// Fetches the current open list of workspace folders.
    ///
    /// Returns `None` if only a single file is open in the tool. Returns an empty `Vec` if a
    /// workspace is open but no folders are configured.
    ///
    /// This corresponds to the [`workspace/workspaceFolders`] request.
    ///
    /// [`workspace/workspaceFolders`]: https://microsoft.github.io/language-server-protocol/specification#workspace_workspaceFolders
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    #[rustfmt::skip]
    pub async fn workspace_folders(&self) -> crate::jsonrpc::Result<Option<Vec<lsp::WorkspaceFolder>>> {
        let token = None;
        self.send_request_initialized::<lsp::request::WorkspaceFoldersRequest>((), token).await
    }

    /// Fetches configuration settings from the client.
    ///
    /// The request can fetch several configuration settings in one roundtrip. The order of the
    /// returned configuration settings correspond to the order of the passed
    /// [`ConfigurationItem`]s (e.g. the first item in the response is the result for the first
    /// configuration item in the params).
    ///
    /// [`ConfigurationItem`]: https://docs.rs/lsp-types/0.74.0/lsp_types/struct.ConfigurationItem.html
    ///
    /// This corresponds to the [`workspace/configuration`] request.
    ///
    /// [`workspace/configuration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_configuration
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    #[rustfmt::skip]
    pub async fn configuration(
        &self,
        items: Vec<lsp::ConfigurationItem>,
    ) -> crate::jsonrpc::Result<Vec<serde_json::Value>> {
        let token = None;
        let params = lsp::ConfigurationParams { items };
        self.send_request_initialized::<lsp::request::WorkspaceConfiguration>(params, token).await
    }

    /// Requests a workspace resource be edited on the client side and returns whether the edit was
    /// applied.
    ///
    /// This corresponds to the [`workspace/applyEdit`] request.
    ///
    /// [`workspace/applyEdit`]: https://microsoft.github.io/language-server-protocol/specification#workspace_applyEdit
    ///
    /// # Initialization
    ///
    /// If the request is sent to client before the server has been initialized, this will
    /// immediately return `Err` with JSON-RPC error code `-32002` ([read more]).
    ///
    /// [read more]: https://microsoft.github.io/language-server-protocol/specification#initialize
    #[rustfmt::skip]
    pub async fn apply_edit(
        &self,
        edit: lsp::WorkspaceEdit,
        label: Option<String>,
    ) -> crate::jsonrpc::Result<lsp::ApplyWorkspaceEditResponse> {
        let token = None;
        let params = lsp::ApplyWorkspaceEditParams { edit, label };
        self.send_request_initialized::<lsp::request::ApplyWorkspaceEdit>(params, token).await
    }

    /// Submits validation diagnostics for an open file with the given URI.
    ///
    /// This corresponds to the [`textDocument/publishDiagnostics`] notification.
    ///
    /// [`textDocument/publishDiagnostics`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_publishDiagnostics
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    #[rustfmt::skip]
    pub async fn publish_diagnostics(&self, uri: lsp::Url, diags: Vec<lsp::Diagnostic>, version: Option<i32>) {
        let params = lsp::PublishDiagnosticsParams::new(uri, diags, version);
        self.send_notification_initialized::<lsp::notification::PublishDiagnostics>(params).await;
    }

    /// Sends a custom notification to the client.
    ///
    /// # Initialization
    ///
    /// This notification will only be sent if the server is initialized.
    pub async fn send_custom_notification<N>(&self, params: N::Params)
    where
        N: lsp::notification::Notification,
    {
        self.send_notification_initialized::<N>(params).await;
    }

    async fn send_notification<N>(&self, params: N::Params)
    where
        N: lsp::notification::Notification,
    {
        let mut sender = self.inner.sender.clone();
        let message = crate::jsonrpc::Outgoing::Request(crate::jsonrpc::ClientRequest::notification::<N>(params));
        if sender.send(message).await.is_err() {
            log::error!("failed to send notification")
        }
    }

    async fn send_notification_initialized<N>(&self, params: N::Params)
    where
        N: lsp::notification::Notification,
    {
        if let crate::server::StateKind::Initialized | crate::server::StateKind::ShutDown = self.inner.state.get() {
            self.send_notification::<N>(params).await;
        } else {
            let msg = crate::jsonrpc::ClientRequest::notification::<N>(params);
            log::trace!("server not initialized, supressing message: {}", msg);
        }
    }

    /// Sends a custom request to the client.
    ///
    /// # Initialization
    ///
    /// This request will only be sent if the server is initialized.
    pub async fn send_custom_request<R>(
        &self,
        params: R::Params,
        token: Option<CancellationToken>,
    ) -> crate::jsonrpc::Result<R::Result>
    where
        R: lsp::request::Request,
    {
        self.send_request_initialized::<R>(params, token).await
    }

    async fn send_request<R>(
        &self,
        params: R::Params,
        token: Option<CancellationToken>,
    ) -> crate::jsonrpc::Result<R::Result>
    where
        R: lsp::request::Request,
    {
        let id = self.inner.request_id.fetch_add(1, Ordering::Relaxed);
        let message = crate::jsonrpc::Outgoing::Request(crate::jsonrpc::ClientRequest::request::<R>(id, params));

        let response_waiter = self.inner.pending_requests.wait(crate::jsonrpc::Id::Number(id));

        if self.inner.sender.clone().send(message).await.is_err() {
            log::error!("failed to send request");
            return Err(crate::jsonrpc::Error::internal_error());
        }

        let mut token_wait = match token {
            None => future::pending().boxed().fuse(),
            Some(token) => token.wait().boxed().fuse(),
        };

        select! {
            _ = token_wait => {
                if self.inner.pending_requests.0.remove(&crate::jsonrpc::Id::Number(id)).is_none() {
                    log::warn!("received response with unknown request ID: {}", id);
                }
                let params = {
                    let id = i32::try_from(id).expect("error converting u64 to i32");
                    lsp::CancelParams { id: lsp::NumberOrString::Number(id) }
                };
                self.send_notification::<lsp::notification::Cancel>(params).await;
                Err(crate::jsonrpc::Error::request_cancelled())
            },
            response = response_waiter.fuse() => {
                let (_, result) = response.into_parts();
                result.and_then(|v| {
                    serde_json::from_value(v).map_err(|e| crate::jsonrpc::Error {
                        code: crate::jsonrpc::ErrorCode::ParseError,
                        message: e.to_string(),
                        data: None,
                    })
                })
            },
        }
    }

    async fn send_request_initialized<R>(
        &self,
        params: R::Params,
        token: Option<CancellationToken>,
    ) -> crate::jsonrpc::Result<R::Result>
    where
        R: lsp::request::Request,
    {
        if let crate::server::StateKind::Initialized | crate::server::StateKind::ShutDown = self.inner.state.get() {
            self.send_request::<R>(params, token).await
        } else {
            let id = self.inner.request_id.load(Ordering::SeqCst) + 1;
            let msg = crate::jsonrpc::ClientRequest::request::<R>(id, params);
            log::trace!("server not initialized, supressing message: {}", msg);
            Err(crate::jsonrpc::not_initialized_error())
        }
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct(stringify!(Client))
            .field("request_id", &self.inner.request_id)
            .field("pending_requests", &self.inner.pending_requests)
            .field("state", &self.inner.state)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod cancellation_token {
        use super::*;

        #[test]
        fn is_cancelled() {
            let mut canceller = TokenCanceller::default();
            let token = canceller.token();
            assert!(!token.is_cancelled());
            canceller.cancel();
            assert!(token.is_cancelled());
        }
    }

    mod client {
        use super::*;
        use crate::jsonrpc::{ClientRequest, Id, Outgoing, Response};
        use futures::StreamExt;
        use serde_json::json;

        mod helper {
            use super::*;
            use crate::jsonrpc::Outgoing;
            use futures::channel::mpsc;

            pub(super) fn client(initialize: bool) -> (Client, mpsc::Receiver<Outgoing>) {
                let state = Arc::new(crate::server::State::new());
                let (tx, rx) = mpsc::channel(4);
                let pending_client = Arc::new(crate::jsonrpc::ClientRequests::new());
                let client = crate::client::Client::new(tx, pending_client, state);
                if initialize {
                    client.inner.state.set(crate::server::StateKind::Initialized);
                }
                (client, rx)
            }
        }

        #[tokio::test]
        async fn apply_edit() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(false);

            let req = {
                let edit = lsp::WorkspaceEdit::default();
                let label = Default::default();
                client.apply_edit(edit, label)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(lsp::ApplyWorkspaceEditResponse {
                    applied: Default::default(),
                    failure_reason: Default::default(),
                    failed_change: Default::default(),
                })
                .unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Err(crate::jsonrpc::not_initialized_error()));

            Ok(())
        }

        #[tokio::test]
        async fn configuration() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(false);

            let req = {
                let items = Default::default();
                client.configuration(items)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(Vec::<serde_json::Value>::new()).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Err(crate::jsonrpc::not_initialized_error()));

            Ok(())
        }

        #[test]
        fn display() {
            let client = helper::client(true).0;
            let _ = format!("{:?}", client);
        }

        #[test]
        fn new() {
            let client = helper::client(false).0;
            assert_eq!(client.inner.state.get(), crate::server::StateKind::Uninitialized);
        }

        #[tokio::test]
        async fn log_message() {
            let (client, mut rx) = helper::client(true);
            let typ = lsp::MessageType::Info;
            let message = String::default();
            client.log_message(typ, message.clone()).await;
            if let Some(item) = rx.next().await {
                let params = lsp::LogMessageParams { typ, message };
                let message = Outgoing::Request(ClientRequest::notification::<lsp::notification::LogMessage>(params));
                assert_eq!(item, message);
            }
        }

        #[tokio::test]
        async fn publish_diagnostics() {
            let (client, mut rx) = helper::client(true);
            let uri = lsp::Url::parse("inmemory::///test").unwrap();
            let diags = Vec::<lsp::Diagnostic>::new();
            let version = Option::<i32>::default();
            client.publish_diagnostics(uri.clone(), diags.clone(), version).await;
            if let Some(item) = rx.next().await {
                let params = lsp::PublishDiagnosticsParams {
                    uri,
                    diagnostics: diags,
                    version,
                };
                let message = Outgoing::Request(ClientRequest::notification::<lsp::notification::PublishDiagnostics>(
                    params,
                ));
                assert_eq!(item, message);
            }
        }

        #[tokio::test]
        async fn register_capability() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(true);

            let req = {
                let registrations = Default::default();
                client.register_capability(registrations)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(()).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Ok(()));

            Ok(())
        }

        #[tokio::test]
        async fn send_notification_initialized_when_uninitialized() {
            let (client, _rx) = helper::client(false);
            let uri = lsp::Url::parse("inmemory::///test").unwrap();
            let diags = Vec::<lsp::Diagnostic>::new();
            let version = Option::<i32>::default();
            client.publish_diagnostics(uri.clone(), diags.clone(), version).await;
        }

        #[tokio::test]
        async fn send_custom_notification() {
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Deserialize, Serialize)]
            struct CustomNotificationParams;

            enum CustomNotification {}

            impl lsp::notification::Notification for CustomNotification {
                type Params = CustomNotificationParams;

                const METHOD: &'static str = "custom/notification";
            }

            let (client, mut rx) = helper::client(true);
            let params = CustomNotificationParams;
            client.send_custom_notification::<CustomNotification>(params).await;

            if let Some(item) = rx.next().await {
                let params = CustomNotificationParams;
                let request = ClientRequest::notification::<CustomNotification>(params);
                let message = Outgoing::Request(request);
                assert_eq!(item, message);
            }
        }

        #[tokio::test]
        async fn send_custom_request() {
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Deserialize, Serialize)]
            struct CustomRequestParams;

            enum CustomRequest {}

            impl lsp::request::Request for CustomRequest {
                type Params = CustomRequestParams;
                type Result = ();

                const METHOD: &'static str = "custom/request";
            }

            let mut canceller = TokenCanceller::new();
            let token = Some(canceller.token());

            let (client, _rx) = helper::client(true);
            let req = {
                let params = CustomRequestParams;
                client.send_custom_request::<CustomRequest>(params, token)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(()).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };

            let cancel = async {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                canceller.cancel();
            };

            let (result, (), ()) = futures::future::join3(req, rsp, cancel).await;
            assert_eq!(result, Ok(()));
        }

        #[tokio::test]
        async fn send_custom_request_with_invalid_response() {
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Deserialize, Serialize)]
            struct CustomRequestParams;

            enum CustomRequest {}

            impl lsp::request::Request for CustomRequest {
                type Params = CustomRequestParams;
                type Result = ();

                const METHOD: &'static str = "custom/request";
            }

            let (client, _rx) = helper::client(true);
            let req = {
                let token = None;
                let params = CustomRequestParams;
                client.send_custom_request::<CustomRequest>(params, token)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(u64::MAX).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };

            let (result, ()) = futures::future::join(req, rsp).await;
            assert!(matches!(
                result,
                Err(crate::jsonrpc::Error {
                    code: crate::jsonrpc::ErrorCode::ParseError,
                    ..
                })
            ));
        }

        #[tokio::test]
        async fn send_custom_request_and_cancel() {
            use serde::{Deserialize, Serialize};

            #[derive(Debug, Deserialize, Serialize)]
            struct CustomRequestParams;

            enum CustomRequest {}

            impl lsp::request::Request for CustomRequest {
                type Params = CustomRequestParams;
                type Result = u64;

                const METHOD: &'static str = "custom/request";
            }

            let mut canceller = TokenCanceller::new();
            let token = Some(canceller.token());

            let (client, _rx) = helper::client(true);
            let req = {
                let params = CustomRequestParams;
                client.send_custom_request::<CustomRequest>(params, token)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(()).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };

            canceller.cancel();

            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Err(crate::jsonrpc::Error::request_cancelled()));
        }

        #[tokio::test]
        async fn show_message() {
            let (client, mut rx) = helper::client(true);
            let typ = lsp::MessageType::Info;
            let message = String::default();
            client.show_message(typ, message.clone()).await;
            if let Some(item) = rx.next().await {
                let params = lsp::ShowMessageParams { typ, message };
                let message = Outgoing::Request(ClientRequest::notification::<lsp::notification::ShowMessage>(params));
                assert_eq!(item, message);
            }
        }

        #[tokio::test]
        async fn show_message_request() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(true);

            let typ = lsp::MessageType::Info;
            let message = String::default();
            let actions = Default::default();

            let req = client.show_message_request(typ, message.clone(), actions);
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(None::<lsp::MessageActionItem>).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert!(result.is_ok());

            Ok(())
        }

        #[tokio::test]
        async fn telemetry_event() {
            let (client, mut rx) = helper::client(true);
            client.telemetry_event(42u8).await;
            if let Some(item) = rx.next().await {
                let params = json!([42u8]);
                let message =
                    Outgoing::Request(ClientRequest::notification::<lsp::notification::TelemetryEvent>(params));
                assert_eq!(item, message);
            }
        }

        #[tokio::test]
        async fn unregister_capability() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(true);

            let req = {
                let unregistrations = Default::default();
                client.unregister_capability(unregistrations)
            };
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(()).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Ok(()));

            Ok(())
        }

        #[tokio::test]
        async fn workspace_folders() -> anyhow::Result<()> {
            let (client, _rx) = helper::client(true);

            let req = client.workspace_folders();
            let rsp = async {
                let id = Id::Number(0);
                let result = serde_json::to_value(None::<Vec<lsp::WorkspaceFolder>>).unwrap();
                client.inner.pending_requests.insert(Response::ok(id, result));
            };
            let (result, ()) = futures::future::join(req, rsp).await;
            assert_eq!(result, Ok(None));

            Ok(())
        }
    }

    mod token_canceller {
        use super::*;

        #[test]
        fn debug() {
            let canceller = TokenCanceller::new();
            format!("{:?}", canceller);
        }

        #[test]
        fn default() {
            let canceller = TokenCanceller::default();
            format!("{:?}", canceller);
        }
    }
}
