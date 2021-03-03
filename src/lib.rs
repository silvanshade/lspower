//! Language Server Protocol (LSP) server abstraction for [Tower].
//!
//! [Tower]: https://github.com/tower-rs/tower

#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub extern crate lsp;

mod client;
mod codec;
pub mod jsonrpc;
mod server;
mod service;
mod transport;

pub use self::{
    client::Client,
    service::{ExitedError, LspService, MessageStream},
    transport::Server,
};
pub use async_trait::async_trait;
use auto_impl::auto_impl;
use lspower_macros::rpc;

/// Trait implemented by language server backends.
///
/// This interface allows servers adhering to the [Language Server Protocol] to be implemented in a
/// safe and easily testable way without exposing the low-level implementation details.
///
/// [Language Server Protocol]: https://microsoft.github.io/language-server-protocol/
#[rpc]
#[async_trait]
#[auto_impl(Arc, Box)]
pub trait LanguageServer: Send + Sync + 'static {
    /// The [`initialize`] request is the first request sent from the client to the server.
    ///
    /// [`initialize`]: https://microsoft.github.io/language-server-protocol/specification#initialize
    ///
    /// This method is guaranteed to only execute once. If the client sends this request to the
    /// server again, the server will respond with JSON-RPC error code `-32600` (invalid request).
    #[rpc(name = "initialize")]
    async fn initialize(&self, params: lsp::InitializeParams) -> crate::jsonrpc::Result<lsp::InitializeResult>;

    /// The [`initialized`] notification is sent from the client to the server after the client
    /// received the result of the initialize request but before the client sends anything else.
    ///
    /// The server can use the `initialized` notification for example to dynamically register
    /// capabilities with the client.
    ///
    /// [`initialized`]: https://microsoft.github.io/language-server-protocol/specification#initialized
    #[rpc(name = "initialized")]
    async fn initialized(&self, _params: lsp::InitializedParams) {
    }

    /// The [`shutdown`] request asks the server to gracefully shut down, but to not exit.
    ///
    /// This request is often later followed by an [`exit`] notification, which will cause the
    /// server to exit immediately.
    ///
    /// [`shutdown`]: https://microsoft.github.io/language-server-protocol/specification#shutdown
    /// [`exit`]: https://microsoft.github.io/language-server-protocol/specification#exit
    ///
    /// This method is guaranteed to only execute once. If the client sends this request to the
    /// server again, the server will respond with JSON-RPC error code `-32600` (invalid request).
    #[rpc(name = "shutdown")]
    async fn shutdown(&self) -> crate::jsonrpc::Result<()>;

    /// The [`workspace/didChangeWorkspaceFolders`] notification is sent from the client to the
    /// server to inform about workspace folder configuration changes.
    ///
    /// The notification is sent by default if both of these boolean fields were set to `true` in
    /// the [`initialize`] method:
    ///
    /// * `InitializeParams::capabilities::workspace::workspace_folders`
    /// * `InitializeResult::capabilities::workspace::workspace_folders::supported`
    ///
    /// This notification is also sent if the server has registered itself to receive this
    /// notification.
    ///
    /// [`workspace/didChangeWorkspaceFolders`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeWorkspaceFolders
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "workspace/didChangeWorkspaceFolders")]
    async fn did_change_workspace_folders(&self, _params: lsp::DidChangeWorkspaceFoldersParams) {
        log::warn!("Got a workspace/didChangeWorkspaceFolders notification, but it is not implemented");
    }

    /// The [`workspace/didChangeConfiguration`] notification is sent from the client to the server
    /// to signal the change of configuration settings.
    ///
    /// [`workspace/didChangeConfiguration`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeConfiguration
    #[rpc(name = "workspace/didChangeConfiguration")]
    async fn did_change_configuration(&self, _params: lsp::DidChangeConfigurationParams) {
        log::warn!("Got a workspace/didChangeConfiguration notification, but it is not implemented");
    }

    /// The [`workspace/didChangeWatchedFiles`] notification is sent from the client to the server
    /// when the client detects changes to files watched by the language client.
    ///
    /// It is recommended that servers register for these file events using the registration
    /// mechanism. This can be done here or in the [`initialized`] method using
    /// `Client::register_capability()`.
    ///
    /// [`workspace/didChangeWatchedFiles`]: https://microsoft.github.io/language-server-protocol/specification#workspace_didChangeConfiguration
    /// [`initialized`]: #tymethod.initialized
    #[rpc(name = "workspace/didChangeWatchedFiles")]
    async fn did_change_watched_files(&self, _params: lsp::DidChangeWatchedFilesParams) {
        log::warn!("Got a workspace/didChangeWatchedFiles notification, but it is not implemented");
    }

    /// The [`workspace/symbol`] request is sent from the client to the server to list project-wide
    /// symbols matching the given query string.
    ///
    /// [`workspace/symbol`]: https://microsoft.github.io/language-server-protocol/specification#workspace_symbol
    #[rpc(name = "workspace/symbol")]
    async fn symbol(
        &self,
        _params: lsp::WorkspaceSymbolParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::SymbolInformation>>> {
        log::error!("Got a workspace/symbol request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`workspace/executeCommand`] request is sent from the client to the server to trigger
    /// command execution on the server.
    ///
    /// In most cases, the server creates a `WorkspaceEdit` structure and applies the changes to
    /// the workspace using `Client::apply_edit()` before returning from this function.
    ///
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specification#workspace_executeCommand
    #[rpc(name = "workspace/executeCommand")]
    async fn execute_command(
        &self,
        _params: lsp::ExecuteCommandParams,
    ) -> crate::jsonrpc::Result<Option<serde_json::Value>> {
        log::error!("Got a workspace/executeCommand request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/didOpen`] notification is sent from the client to the server to signal
    /// that a new text document has been opened by the client.
    ///
    /// The document's truth is now managed by the client and the server must not try to read the
    /// document’s truth using the document's URI. "Open" in this sense means it is managed by the
    /// client. It doesn't necessarily mean that its content is presented in an editor.
    ///
    /// [`textDocument/didOpen`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didOpen
    #[rpc(name = "textDocument/didOpen")]
    async fn did_open(&self, _params: lsp::DidOpenTextDocumentParams) {
        log::warn!("Got a textDocument/didOpen notification, but it is not implemented");
    }

    /// The [`textDocument/didChange`] notification is sent from the client to the server to signal
    /// changes to a text document.
    ///
    /// This notification will contain a distinct version tag and a list of edits made to the
    /// document for the server to interpret.
    ///
    /// [`textDocument/didChange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didChange
    #[rpc(name = "textDocument/didChange")]
    async fn did_change(&self, _params: lsp::DidChangeTextDocumentParams) {
        log::warn!("Got a textDocument/didChange notification, but it is not implemented");
    }

    /// The [`textDocument/willSave`] notification is sent from the client to the server before the
    /// document is actually saved.
    ///
    /// [`textDocument/willSave`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_willSave
    #[rpc(name = "textDocument/willSave")]
    async fn will_save(&self, _params: lsp::WillSaveTextDocumentParams) {
        log::warn!("Got a textDocument/willSave notification, but it is not implemented");
    }

    /// The [`textDocument/willSaveWaitUntil`] request is sent from the client to the server before
    /// the document is actually saved.
    ///
    /// The request can return an array of `TextEdit`s which will be applied to the text document
    /// before it is saved.
    ///
    /// Please note that clients might drop results if computing the text edits took too long or if
    /// a server constantly fails on this request. This is done to keep the save fast and reliable.
    #[rpc(name = "textDocument/willSaveWaitUntil")]
    async fn will_save_wait_until(
        &self,
        _params: lsp::WillSaveTextDocumentParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::TextEdit>>> {
        log::error!("Got a textDocument/willSaveWaitUntil request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/didSave`] notification is sent from the client to the server when the
    /// document was saved in the client.
    ///
    /// [`textDocument/didSave`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didSave
    #[rpc(name = "textDocument/didSave")]
    async fn did_save(&self, _params: lsp::DidSaveTextDocumentParams) {
        log::warn!("Got a textDocument/didSave notification, but it is not implemented");
    }

    /// The [`textDocument/didClose`] notification is sent from the client to the server when the
    /// document got closed in the client.
    ///
    /// The document's truth now exists where the document's URI points to (e.g. if the document's
    /// URI is a file URI, the truth now exists on disk).
    ///
    /// [`textDocument/didClose`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_didClose
    #[rpc(name = "textDocument/didClose")]
    async fn did_close(&self, _params: lsp::DidCloseTextDocumentParams) {
        log::warn!("Got a textDocument/didClose notification, but it is not implemented");
    }

    /// The [`textDocument/completion`] request is sent from the client to the server to compute
    /// completion items at a given cursor position.
    ///
    /// If computing full completion items is expensive, servers can additionally provide a handler
    /// for the completion item resolve request (`completionItem/resolve`). This request is sent
    /// when a completion item is selected in the user interface.
    ///
    /// [`textDocument/completion`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_completion
    #[rpc(name = "textDocument/completion")]
    async fn completion(
        &self,
        _params: lsp::CompletionParams,
    ) -> crate::jsonrpc::Result<Option<lsp::CompletionResponse>> {
        log::error!("Got a textDocument/completion request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`completionItem/resolve`] request is sent from the client to the server to resolve
    /// additional information for a given completion item.
    ///
    /// [`completionItem/resolve`]: https://microsoft.github.io/language-server-protocol/specification#completionItem_resolve
    #[rpc(name = "completionItem/resolve")]
    async fn completion_resolve(&self, _params: lsp::CompletionItem) -> crate::jsonrpc::Result<lsp::CompletionItem> {
        log::error!("Got a completionItem/resolve request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/hover`] request asks the server for hover information at a given text
    /// document position.
    ///
    /// Such hover information typically includes type signature information and inline
    /// documentation for the symbol at the given text document position.
    ///
    /// [`textDocument/hover`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_hover
    #[rpc(name = "textDocument/hover")]
    async fn hover(&self, _params: lsp::HoverParams) -> crate::jsonrpc::Result<Option<lsp::Hover>> {
        log::error!("Got a textDocument/hover request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/signatureHelp`] request is sent from the client to the server to request
    /// signature information at a given cursor position.
    ///
    /// [`textDocument/signatureHelp`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_signatureHelp
    #[rpc(name = "textDocument/signatureHelp")]
    async fn signature_help(
        &self,
        _params: lsp::SignatureHelpParams,
    ) -> crate::jsonrpc::Result<Option<lsp::SignatureHelp>> {
        log::error!("Got a textDocument/signatureHelp request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/declaration`] request asks the server for the declaration location of a
    /// symbol at a given text document position.
    ///
    /// [`textDocument/declaration`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_declaration
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.14.0.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::declaration::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "textDocument/declaration")]
    async fn goto_declaration(
        &self,
        _params: lsp::request::GotoDeclarationParams,
    ) -> crate::jsonrpc::Result<Option<lsp::request::GotoDeclarationResponse>> {
        log::error!("Got a textDocument/declaration request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/definition`] request asks the server for the definition location of a
    /// symbol at a given text document position.
    ///
    /// [`textDocument/definition`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_definition
    ///
    /// # Compatibility
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::definition::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "textDocument/definition")]
    async fn goto_definition(
        &self,
        _params: lsp::GotoDefinitionParams,
    ) -> crate::jsonrpc::Result<Option<lsp::GotoDefinitionResponse>> {
        log::error!("Got a textDocument/definition request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/typeDefinition`] request asks the server for the type definition location
    /// of a symbol at a given text document position.
    ///
    /// [`textDocument/typeDefinition`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_typeDefinition
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// The [`GotoDefinitionResponse::Link`] return value was introduced in specification version
    /// 3.14.0 and requires client-side support in order to be used. It can be returned if the
    /// client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::type_definition::link_support
    /// ```
    ///
    /// [`GotoDefinitionResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "textDocument/typeDefinition")]
    async fn goto_type_definition(
        &self,
        _params: lsp::request::GotoTypeDefinitionParams,
    ) -> crate::jsonrpc::Result<Option<lsp::request::GotoTypeDefinitionResponse>> {
        log::error!("Got a textDocument/typeDefinition request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/implementation`] request is sent from the client to the server to resolve
    /// the implementation location of a symbol at a given text document position.
    ///
    /// [`textDocument/implementation`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_implementation
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// The [`GotoImplementationResponse::Link`] return value was introduced in specification
    /// version 3.14.0 and requires client-side support in order to be used. It can be returned if
    /// the client set the following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::implementation::link_support
    /// ```
    ///
    /// [`GotoImplementationResponse::Link`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.GotoDefinitionResponse.html#variant.Link
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "textDocument/implementation")]
    async fn goto_implementation(
        &self,
        _params: lsp::request::GotoImplementationParams,
    ) -> crate::jsonrpc::Result<Option<lsp::request::GotoImplementationResponse>> {
        log::error!("Got a textDocument/implementation request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/references`] request is sent from the client to the server to resolve
    /// project-wide references for the symbol denoted by the given text document position.
    ///
    /// [`textDocument/references`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_references
    #[rpc(name = "textDocument/references")]
    async fn references(&self, _params: lsp::ReferenceParams) -> crate::jsonrpc::Result<Option<Vec<lsp::Location>>> {
        log::error!("Got a textDocument/references request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/documentHighlight`] request is sent from the client to the server to
    /// resolve appropriate highlights for a given text document position.
    ///
    /// For programming languages, this usually highlights all textual references to the symbol
    /// scoped to this file.
    ///
    /// This request differs slightly from `textDocument/references` in that this one is allowed to
    /// be more fuzzy.
    ///
    /// [`textDocument/documentHighlight`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentHighlight
    #[rpc(name = "textDocument/documentHighlight")]
    async fn document_highlight(
        &self,
        _params: lsp::DocumentHighlightParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::DocumentHighlight>>> {
        log::error!("Got a textDocument/documentHighlight request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/documentSymbol`] request is sent from the client to the server to
    /// retrieve all symbols found in a given text document.
    ///
    /// The returned result is either:
    ///
    /// * [`DocumentSymbolResponse::Flat`] which is a flat list of all symbols found in a given text
    ///   document. Then neither the symbol’s location range nor the symbol’s container name should
    ///   be used to infer a hierarchy.
    /// * [`DocumentSymbolResponse::Nested`] which is a hierarchy of symbols found in a given text
    ///   document.
    ///
    /// [`textDocument/documentSymbol`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentSymbol
    /// [`DocumentSymbolResponse::Flat`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.DocumentSymbolResponse.html#variant.Flat
    /// [`DocumentSymbolResponse::Nested`]: https://docs.rs/lsp-types/0.74.0/lsp_types/enum.DocumentSymbolResponse.html#variant.Nested
    #[rpc(name = "textDocument/documentSymbol")]
    async fn document_symbol(
        &self,
        _params: lsp::DocumentSymbolParams,
    ) -> crate::jsonrpc::Result<Option<lsp::DocumentSymbolResponse>> {
        log::error!("Got a textDocument/documentSymbol request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/codeAction`] request is sent from the client to the server to compute
    /// commands for a given text document and range. These commands are typically code fixes to
    /// either fix problems or to beautify/refactor code.
    ///
    /// The result of a [`textDocument/codeAction`] request is an array of `Command` literals which
    /// are typically presented in the user interface.
    ///
    /// To ensure that a server is useful in many clients, the commands specified in a code actions
    /// should be handled by the server and not by the client (see [`workspace/executeCommand`] and
    /// `ServerCapabilities::execute_command_provider`). If the client supports providing edits
    /// with a code action, then the mode should be used.
    ///
    /// When the command is selected the server should be contacted again (via the
    /// [`workspace/executeCommand`] request) to execute the command.
    ///
    /// # Compatibility
    ///
    /// Since version 3.8.0: support for `CodeAction` literals to enable the following scenarios:
    ///
    /// * The ability to directly return a workspace edit from the code action request. This avoids
    ///   having another server roundtrip to execute an actual code action. However server providers
    ///   should be aware that if the code action is expensive to compute or the edits are huge it
    ///   might still be beneficial if the result is simply a command and the actual edit is only
    ///   computed when needed.
    ///
    /// * The ability to group code actions using a kind. Clients are allowed to ignore that
    ///   information. However it allows them to better group code action for example into
    ///   corresponding menus (e.g. all refactor code actions into a refactor menu).
    ///
    /// [`textDocument/codeAction`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeAction
    /// [`workspace/executeCommand`]: https://microsoft.github.io/language-server-protocol/specification#workspace_executeCommand
    #[rpc(name = "textDocument/codeAction")]
    async fn code_action(
        &self,
        _params: lsp::CodeActionParams,
    ) -> crate::jsonrpc::Result<Option<lsp::CodeActionResponse>> {
        log::error!("Got a textDocument/codeAction request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/codeLens`] request is sent from the client to the server to compute code
    /// lenses for a given text document.
    ///
    /// [`textDocument/codeLens`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_codeLens
    #[rpc(name = "textDocument/codeLens")]
    async fn code_lens(&self, _params: lsp::CodeLensParams) -> crate::jsonrpc::Result<Option<Vec<lsp::CodeLens>>> {
        log::error!("Got a textDocument/codeLens request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`codeLens/resolve`] request is sent from the client to the server to resolve the
    /// command for a given code lens item.
    ///
    /// [`codeLens/resolve`]: https://microsoft.github.io/language-server-protocol/specification#codeLens_resolve
    #[rpc(name = "codeLens/resolve")]
    async fn code_lens_resolve(&self, _params: lsp::CodeLens) -> crate::jsonrpc::Result<lsp::CodeLens> {
        log::error!("Got a codeLens/resolve request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/documentLink`] request is sent from the client to the server to request
    /// the location of links in a document.
    ///
    /// A document link is a range in a text document that links to an internal or external
    /// resource, like another text document or a web site.
    ///
    /// [`textDocument/documentLink`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentLink
    ///
    /// # Compatibility
    ///
    /// The [`DocumentLink::tooltip`] field was introduced in specification version 3.15.0 and
    /// requires client-side support in order to be used. It can be returned if the client set the
    /// following field to `true` in the [`initialize`] method:
    ///
    /// ```text
    /// InitializeParams::capabilities::text_document::document_link::tooltip_support
    /// ```
    ///
    /// [`initialize`]: #tymethod.initialize
    #[rpc(name = "textDocument/documentLink")]
    async fn document_link(
        &self,
        _params: lsp::DocumentLinkParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::DocumentLink>>> {
        log::error!("Got a textDocument/documentLink request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`documentLink/resolve`] request is sent from the client to the server to resolve the
    /// target of a given document link.
    ///
    /// A document link is a range in a text document that links to an internal or external
    /// resource, like another text document or a web site.
    ///
    /// [`documentLink/resolve`]: https://microsoft.github.io/language-server-protocol/specification#documentLink_resolve
    #[rpc(name = "documentLink/resolve")]
    async fn document_link_resolve(&self, _params: lsp::DocumentLink) -> crate::jsonrpc::Result<lsp::DocumentLink> {
        log::error!("Got a documentLink/resolve request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/documentColor`] request is sent from the client to the server to list
    /// all color references found in a given text document. Along with the range, a color value in
    /// RGB is returned.
    ///
    /// [`textDocument/documentColor`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_documentColor
    ///
    /// Clients can use the result to decorate color references in an editor. For example:
    ///
    /// * Color boxes showing the actual color next to the reference
    /// * Show a color picker when a color reference is edited
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    #[rpc(name = "textDocument/documentColor")]
    async fn document_color(
        &self,
        _params: lsp::DocumentColorParams,
    ) -> crate::jsonrpc::Result<Vec<lsp::ColorInformation>> {
        log::error!("Got a textDocument/documentColor request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/colorPresentation`] request is sent from the client to the server to
    /// obtain a list of presentations for a color value at a given location.
    ///
    /// Clients can use the result to:
    ///
    /// * Modify a color reference
    /// * Show in a color picker and let users pick one of the presentations
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.6.0.
    ///
    /// This request has no special capabilities and registration options since it is sent as a
    /// resolve request for the [`textDocument/documentColor`] request.
    ///
    /// [`textDocument/documentColor`]: #tymethod.document_color
    #[rpc(name = "textDocument/colorPresentation")]
    async fn color_presentation(
        &self,
        _params: lsp::ColorPresentationParams,
    ) -> crate::jsonrpc::Result<Vec<lsp::ColorPresentation>> {
        log::error!("Got a textDocument/colorPresentation request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/formatting`] request is sent from the client to the server to format a
    /// whole document.
    ///
    /// [`textDocument/formatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_formatting
    #[rpc(name = "textDocument/formatting")]
    async fn formatting(
        &self,
        _params: lsp::DocumentFormattingParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::TextEdit>>> {
        log::error!("Got a textDocument/formatting request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/rangeFormatting`] request is sent from the client to the server to
    /// format a given range in a document.
    ///
    /// [`textDocument/rangeFormatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rangeFormatting
    #[rpc(name = "textDocument/rangeFormatting")]
    async fn range_formatting(
        &self,
        _params: lsp::DocumentRangeFormattingParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::TextEdit>>> {
        log::error!("Got a textDocument/rangeFormatting request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/onTypeFormatting`] request is sent from the client to the server to
    /// format parts of the document during typing.
    ///
    /// [`textDocument/onTypeFormatting`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_onTypeFormatting
    #[rpc(name = "textDocument/onTypeFormatting")]
    async fn on_type_formatting(
        &self,
        _params: lsp::DocumentOnTypeFormattingParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::TextEdit>>> {
        log::error!("Got a textDocument/onTypeFormatting request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/rename`] request is sent from the client to the server to ask the server
    /// to compute a workspace change so that the client can perform a workspace-wide rename of a
    /// symbol.
    ///
    /// [`textDocument/rename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_rename
    #[rpc(name = "textDocument/rename")]
    async fn rename(&self, _params: lsp::RenameParams) -> crate::jsonrpc::Result<Option<lsp::WorkspaceEdit>> {
        log::error!("Got a textDocument/rename request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/prepareRename`] request is sent from the client to the server to setup
    /// and test the validity of a rename operation at a given location.
    ///
    /// [`textDocument/prepareRename`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_prepareRename
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.12.0.
    #[rpc(name = "textDocument/prepareRename")]
    async fn prepare_rename(
        &self,
        _params: lsp::TextDocumentPositionParams,
    ) -> crate::jsonrpc::Result<Option<lsp::PrepareRenameResponse>> {
        log::error!("Got a textDocument/prepareRename request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/foldingRange`] request is sent from the client to the server to return
    /// all folding ranges found in a given text document.
    ///
    /// [`textDocument/foldingRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_foldingRange
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.10.0.
    #[rpc(name = "textDocument/foldingRange")]
    async fn folding_range(
        &self,
        _params: lsp::FoldingRangeParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::FoldingRange>>> {
        log::error!("Got a textDocument/foldingRange request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// The [`textDocument/selectionRange`] request is sent from the client to the server to return
    /// suggested selection ranges at an array of given positions. A selection range is a range
    /// around the cursor position which the user might be interested in selecting.
    ///
    /// [`textDocument/selectionRange`]: https://microsoft.github.io/language-server-protocol/specification#textDocument_selectionRange
    ///
    /// A selection range in the return array is for the position in the provided parameters at the
    /// same index. Therefore `params.positions[i]` must be contained in `result[i].range`.
    ///
    /// # Compatibility
    ///
    /// This request was introduced in specification version 3.15.0.
    #[rpc(name = "textDocument/selectionRange")]
    async fn selection_range(
        &self,
        _params: lsp::SelectionRangeParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::SelectionRange>>> {
        log::error!("Got a textDocument/selectionRange request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`callHierarchy/incomingCalls`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#callHierarchy_incomingCalls
    #[rpc(name = "callHierarchy/incomingCalls")]
    async fn incoming_calls(
        &self,
        _params: lsp::CallHierarchyIncomingCallsParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::CallHierarchyIncomingCall>>> {
        log::error!("Got a callHierarchy/incomingCalls request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`callHierarchy/outgoingCalls`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#callHierarchy_outgoingCalls
    #[rpc(name = "callHierarchy/outgoingCalls")]
    async fn outgoing_calls(
        &self,
        _params: lsp::CallHierarchyOutgoingCallsParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::CallHierarchyOutgoingCall>>> {
        log::error!("Got a callHierarchy/outgoingCalls request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`textDocument/prepareCallHierarchy`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#textDocument_prepareCallHierarchy
    #[rpc(name = "textDocument/prepareCallHierarchy")]
    async fn prepare_call_hierarchy(
        &self,
        _params: lsp::CallHierarchyPrepareParams,
    ) -> crate::jsonrpc::Result<Option<Vec<lsp::CallHierarchyItem>>> {
        log::error!("Got a textDocument/prepareCallHierarchy request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`textDocument/semanticTokens/full`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#textDocument_semanticTokens
    #[rpc(name = "textDocument/semanticTokens/full")]
    async fn semantic_tokens_full(
        &self,
        _params: lsp::SemanticTokensParams,
    ) -> crate::jsonrpc::Result<Option<lsp::SemanticTokensResult>> {
        log::error!("Got a textDocument/semanticTokens/full request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`textDocument/semanticTokens/full/delta`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#textDocument_semanticTokens
    #[rpc(name = "textDocument/semanticTokens/full/delta")]
    async fn semantic_tokens_full_delta(
        &self,
        _params: lsp::SemanticTokensDeltaParams,
    ) -> crate::jsonrpc::Result<Option<lsp::SemanticTokensFullDeltaResult>> {
        log::error!("Got a textDocument/semanticTokens/full/delta request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`textDocument/semanticTokens/range`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#textDocument_semanticTokens
    #[rpc(name = "textDocument/semanticTokens/range")]
    async fn semantic_tokens_range(
        &self,
        _params: lsp::SemanticTokensRangeParams,
    ) -> crate::jsonrpc::Result<Option<lsp::SemanticTokensRangeResult>> {
        log::error!("Got a textDocument/semanticTokens/range request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`workspace/semanticTokens/full`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#textDocument_semanticTokens
    #[rpc(name = "workspace/semanticTokens/refresh")]
    async fn semantic_tokens_refresh(&self) -> crate::jsonrpc::Result<()> {
        log::error!("Got a workspace/semanticTokens/refresh request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// [`codeAction/resolve`]: https://microsoft.github.io/language-server-protocol/specifications/specification-3-16/#codeAction_resolve
    #[rpc(name = "codeAction/resolve")]
    async fn code_action_resolve(&self, _params: lsp::CodeAction) -> crate::jsonrpc::Result<lsp::CodeAction> {
        log::error!("Got a codeAction/resolve request, but it is not implemented");
        Err(crate::jsonrpc::Error::method_not_found())
    }

    /// This handler can be used to respond to all requests that are not handled by built in request
    /// handlers.
    async fn request_else(
        &self,
        method: &str,
        _params: Option<serde_json::Value>,
    ) -> crate::jsonrpc::Result<Option<serde_json::Value>> {
        log::error!(
            "Got a {} request, but LanguageServer::request_else is not implemented",
            method
        );
        Err(crate::jsonrpc::Error::method_not_found())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsonrpc::Incoming;
    use serde_json::json;
    use std::task::Poll;
    use tower_test::mock::Spawn;

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

    mod helper {
        use super::*;
        use crate::jsonrpc::Incoming;
        use serde_json::json;
        use std::task::Poll;
        use tower_test::mock::Spawn;

        pub(super) async fn initialize(service: &mut Spawn<LspService>) {
            let params = serde_json::from_value::<lsp::InitializeParams>(json!({ "capabilities": {} })).unwrap();
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "initialize",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response =
                serde_json::from_value(json!({ "jsonrpc": "2.0", "result": { "capabilities": {} }, "id": 1 })).unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(Some(response)));
        }
    }

    #[tokio::test]
    async fn initialized() {
        let (service, _) = LspService::new(|_| Mock::default());
        let mut service = Spawn::new(service);

        helper::initialize(&mut service).await;

        let params = lsp::InitializedParams {};
        let request: Incoming = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": params,
            "id": 1,
        }))
        .unwrap();
        assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
        assert_eq!(service.call(request.clone()).await, Ok(None));
    }

    mod completion_item {
        use super::*;
        use crate::jsonrpc::{Error, Id, Incoming, Outgoing, Response};
        use serde_json::json;
        use std::task::Poll;
        use tower_test::mock::Spawn;

        #[tokio::test]
        async fn resolve() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::CompletionItem::default();
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "completionItem/resolve",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }
    }

    mod text_document {
        use super::*;
        use crate::jsonrpc::{Error, Id, Incoming, Outgoing, Response};
        use serde_json::json;
        use std::task::Poll;
        use tower_test::mock::Spawn;

        #[tokio::test]
        async fn completion() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::CompletionParams {
                text_document_position: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/completion",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn declaration() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::request::GotoDeclarationParams {
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/declaration",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn definition() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::GotoDefinitionParams {
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/definition",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn did_change() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidChangeTextDocumentParams {
                text_document: lsp::VersionedTextDocumentIdentifier {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    version: Default::default(),
                },
                content_changes: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn did_close() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidCloseTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                },
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didClose",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn did_open() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidOpenTextDocumentParams {
                text_document: lsp::TextDocumentItem {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    language_id: Default::default(),
                    version: Default::default(),
                    text: Default::default(),
                },
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn did_save() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidSaveTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                },
                text: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn hover() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::HoverParams {
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/hover",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn implementation() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::request::GotoImplementationParams {
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/implementation",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn references() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::ReferenceParams {
                text_document_position: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: lsp::ReferenceContext {
                    include_declaration: Default::default(),
                },
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/references",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn signature_help() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::SignatureHelpParams {
                context: Default::default(),
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/signatureHelp",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn type_definition() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::request::GotoTypeDefinitionParams {
                text_document_position_params: lsp::TextDocumentPositionParams {
                    text_document: lsp::TextDocumentIdentifier {
                        uri: lsp::Url::parse("inmemory::///test").unwrap(),
                    },
                    position: Default::default(),
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/typeDefinition",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn will_save() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::WillSaveTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                },
                reason: lsp::TextDocumentSaveReason::Manual,
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/willSave",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn will_save_wait_until() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::WillSaveTextDocumentParams {
                text_document: lsp::TextDocumentIdentifier {
                    uri: lsp::Url::parse("inmemory::///test").unwrap(),
                },
                reason: lsp::TextDocumentSaveReason::Manual,
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "textDocument/willSaveWaitUntil",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }
    }

    mod workspace {
        use super::*;
        use crate::jsonrpc::{Error, Id, Incoming, Outgoing, Response};
        use serde_json::{json, Value};
        use std::task::Poll;
        use tower_test::mock::Spawn;

        #[tokio::test]
        async fn did_change_configuration() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidChangeConfigurationParams { settings: Value::Null };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "workspace/didChangeConfiguration",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn did_change_watched_files() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidChangeWatchedFilesParams {
                changes: Default::default(),
            };
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "workspace/didChangeWatchedFiles",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn did_change_workspace_folders() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::DidChangeWorkspaceFoldersParams::default();
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "workspace/didChangeWorkspaceFolders",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(service.call(request.clone()).await, Ok(None));
        }

        #[tokio::test]
        async fn execute_command() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::ExecuteCommandParams::default();
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "workspace/executeCommand",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }

        #[tokio::test]
        async fn symbol() {
            let (service, _) = LspService::new(|_| Mock::default());
            let mut service = Spawn::new(service);

            super::helper::initialize(&mut service).await;

            let params = lsp::WorkspaceSymbolParams::default();
            let request: Incoming = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "workspace/symbol",
                "params": params,
                "id": 1,
            }))
            .unwrap();
            let response = Response::error(Some(Id::Number(1)), Error::method_not_found());
            assert_eq!(service.poll_ready(), Poll::Ready(Ok(())));
            assert_eq!(
                service.call(request.clone()).await,
                Ok(Some(Outgoing::Response(response)))
            );
        }
    }
}
