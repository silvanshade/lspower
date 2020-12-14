# lspower

[![Build Status][build-badge]][build-url]
[![Crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]

[build-badge]: https://github.com/rslint/lspower/workflows/rust/badge.svg
[build-url]: https://github.com/rslint/lspower/actions
[crates-badge]: https://img.shields.io/crates/v/lspower.svg
[crates-url]: https://crates.io/crates/lspower
[docs-badge]: https://docs.rs/lspower/badge.svg
[docs-url]: https://docs.rs/lspower

[Language Server Protocol] implementation for Rust based on [Tower].

[Language Server Protocol]: https://microsoft.github.io/language-server-protocol
[Tower]: https://github.com/tower-rs/tower

Tower is a simple and composable framework for implementing asynchronous
services in Rust. Central to Tower is the [`Service`] trait, which provides the
necessary abstractions for defining request/response clients and servers.
Examples of protocols implemented using the `Service` trait include
[`hyper`] for HTTP and [`tonic`] for gRPC.

[`Service`]: https://docs.rs/tower-service/
[`hyper`]: https://docs.rs/hyper/
[`tonic`]: https://docs.rs/tonic/

This library (`lspower`) provides a simple implementation of the Language
Server Protocol (LSP) that makes it easy to write your own language server. It
consists of three parts:

* The `LanguageServer` trait which defines the behavior of your language server.
* The asynchronous `LspService` delegate which wraps your language server
  implementation and defines the behavior of the protocol.
* A `Server` which spawns the `LspService` and processes requests and responses
  over `stdio` or TCP.

## Example

```rust
use lspower::jsonrpc::Result;
use lspower::lsp_types::*;
use lspower::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
}

#[lspower::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult::default())
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::Info, "server initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, messages) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout)
        .interleave(messages)
        .serve(service)
        .await;
}
```

## License

`lspower` is free and open source software distributed under the terms of
either the [MIT](LICENSE-MIT) or the [Apache 2.0](LICENSE-APACHE) license, at
your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Acknowledgements

`lspower` is a fork of the [`tower-lsp`](https://github.com/ebkalderon/tower-lsp) crate.
