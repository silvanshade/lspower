use lspower::{
    jsonrpc::{Error, Result},
    lsp::{request::Request, *},
    Client,
    LanguageServer,
    LspService,
    Server,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize)]
struct CustomRequestParams {
    title: String,
    message: String,
}

impl CustomRequestParams {
    fn new(title: impl Into<String>, message: impl Into<String>) -> Self {
        CustomRequestParams {
            title: title.into(),
            message: message.into(),
        }
    }
}

enum CustomRequest {}

impl Request for CustomRequest {
    type Params = CustomRequestParams;
    type Result = String;

    const METHOD: &'static str = "custom/request";
}

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[lspower::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["custom.request".to_string()],
                    work_done_progress_options: Default::default(),
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn execute_command(&self, params: ExecuteCommandParams) -> Result<Option<Value>> {
        if params.command == "custom.request" {
            let result = {
                let params = CustomRequestParams::new("Hello", "Message");
                let token = None;
                self.client.send_custom_request::<CustomRequest>(params, token).await?
            };
            self.client
                .log_message(
                    MessageType::Info,
                    format!("Command executed with params: {:?}, result: {}", params, result),
                )
                .await;
            Ok(None)
        } else {
            Err(Error::invalid_request())
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout).interleave(messages).serve(service).await;
}
