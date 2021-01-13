//! Types for sending data to and from the language client.

use std::{
    fmt::{self, Debug, Formatter},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

struct ClientInner {
    sender: async_channel::Sender<crate::jsonrpc::Outgoing>,
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
        sender: async_channel::Sender<crate::jsonrpc::Outgoing>,
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
        self.send_notification::<lsp::notification::LogMessage>(lsp::LogMessageParams {
            typ,
            message: message.to_string(),
        })
        .await;
    }

    /// Notifies the client to display a particular message in the user interface.
    ///
    /// This corresponds to the [`window/showMessage`] notification.
    ///
    /// [`window/showMessage`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessage
    pub async fn show_message<M: std::fmt::Display>(&self, typ: lsp::MessageType, message: M) {
        self.send_notification::<lsp::notification::ShowMessage>(lsp::ShowMessageParams {
            typ,
            message: message.to_string(),
        })
        .await;
    }

    /// Requests the client to display a particular message in the user interface.
    ///
    /// Unlike the `show_message` notification, this request can also pass a list of actions and
    /// wait for an answer from the client.
    ///
    /// This corresponds to the [`window/showMessageRequest`] request.
    ///
    /// [`window/showMessageRequest`]: https://microsoft.github.io/language-server-protocol/specification#window_showMessageRequest
    pub async fn show_message_request<M: std::fmt::Display>(
        &self,
        typ: lsp::MessageType,
        message: M,
        actions: Option<Vec<lsp::MessageActionItem>>,
    ) -> crate::jsonrpc::Result<Option<lsp::MessageActionItem>> {
        self.send_request::<lsp::request::ShowMessageRequest>(lsp::ShowMessageRequestParams {
            typ,
            message: message.to_string(),
            actions,
        })
        .await
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
    pub async fn register_capability(&self, registrations: Vec<lsp::Registration>) -> crate::jsonrpc::Result<()> {
        self.send_request_initialized::<lsp::request::RegisterCapability>(lsp::RegistrationParams { registrations })
            .await
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
    pub async fn unregister_capability(
        &self,
        unregisterations: Vec<lsp::Unregistration>,
    ) -> crate::jsonrpc::Result<()> {
        self.send_request_initialized::<lsp::request::UnregisterCapability>(lsp::UnregistrationParams {
            unregisterations,
        })
        .await
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
    pub async fn workspace_folders(&self) -> crate::jsonrpc::Result<Option<Vec<lsp::WorkspaceFolder>>> {
        self.send_request_initialized::<lsp::request::WorkspaceFoldersRequest>(())
            .await
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
    pub async fn configuration(
        &self,
        items: Vec<lsp::ConfigurationItem>,
    ) -> crate::jsonrpc::Result<Vec<serde_json::Value>> {
        self.send_request_initialized::<lsp::request::WorkspaceConfiguration>(lsp::ConfigurationParams { items })
            .await
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
    pub async fn apply_edit(
        &self,
        edit: lsp::WorkspaceEdit,
        label: Option<String>,
    ) -> crate::jsonrpc::Result<lsp::ApplyWorkspaceEditResponse> {
        self.send_request_initialized::<lsp::request::ApplyWorkspaceEdit>(lsp::ApplyWorkspaceEditParams { edit, label })
            .await
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
    pub async fn publish_diagnostics(&self, uri: lsp::Url, diags: Vec<lsp::Diagnostic>, version: Option<i32>) {
        self.send_notification_initialized::<lsp::notification::PublishDiagnostics>(
            lsp::PublishDiagnosticsParams::new(uri, diags, version),
        )
        .await;
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
        let sender = self.inner.sender.clone();
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

    async fn send_request<R>(&self, params: R::Params) -> crate::jsonrpc::Result<R::Result>
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

        let response = response_waiter.await;
        let (_, result) = response.into_parts();
        result.and_then(|v| {
            serde_json::from_value(v).map_err(|e| crate::jsonrpc::Error {
                code: crate::jsonrpc::ErrorCode::ParseError,
                message: e.to_string(),
                data: None,
            })
        })
    }

    async fn send_request_initialized<R>(&self, params: R::Params) -> crate::jsonrpc::Result<R::Result>
    where
        R: lsp::request::Request,
    {
        if let crate::server::StateKind::Initialized | crate::server::StateKind::ShutDown = self.inner.state.get() {
            self.send_request::<R>(params).await
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
