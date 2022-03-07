<div align="center">
  <h1><code>lspower</code></h1>
  <p>
    <strong>A <a href="https://microsoft.github.io/language-server-protocol">Language Server Protocol</a>
      implementation for Rust based on <a href="https://github.com/tower-rs/tower">Tower</a></strong>
  </p>
  <p style="margin-bottom: 0.5ex;">
    <a href="https://silvanshade.github.io/lspower/lspower"><img
        src="https://img.shields.io/badge/docs-latest-blueviolet?logo=Read-the-docs&logoColor=white" /></a>
    <a href="https://github.com/silvanshade/lspower/actions"><img
        src="https://github.com/silvanshade/lspower/workflows/main/badge.svg" /></a>
    <a href="https://codecov.io/gh/silvanshade/lspower"><img
        src="https://codecov.io/gh/silvanshade/lspower/branches/main/graph/badge.svg" /></a>
  </p>
</div>

## Note

__This crate has been re-merged with [tower-lsp](https://github.com/ebkalderon/tower-lsp) and is now deprecated.__

## Description

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
* The `LspService` delegate wrapping your server and which defines the protocol.
* A `Server` which spawns `LspService` and processes messages over `stdio` or TCP.

## Example

```rust
use lspower::jsonrpc::Result;
use lspower::lsp::*;
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
            .log_message(MessageType::INFO, "server initialized!")
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

## Differences with tower-lsp

`lspower` is a fork of the [`tower-lsp`](https://github.com/ebkalderon/tower-lsp) crate.

The main differences between these crates are the following:

* `lspower` is currently maintained while `tower-lsp` development seems to have stopped
* `lspower` has had several significant refactorings and bug-fixes since the fork
* `lspower` supports the current LSP spec including more features like semantic tokens
* `lspower` supports sending custom requests from server to client
* `lspower` supports cancellation tokens (and server to client `$/cancelRequest` notifications)
* `lspower` doesn't *require* `tokio` but also works with `async-std`, `smol`, and `futures`
* `lspower` is compatible with WASM targets (resolving: [tower-lsp#187](https://github.com/ebkalderon/tower-lsp/issues/187))
* `lspower` has fewer dependencies (from replacing `nom` with `httparse`)
* `lspower` parses message streams more efficiently and minimizes unnecessary reparsing
* `lspower` recovers faster from malformed messages (SIMD accelerated via `twoway`)

## Using lspower with runtimes other than tokio

By default, `lspower` is configured for use with `tokio`.

Using `lspower` with other runtimes requires disabling `default-features` and
enabling the `runtime-agnostic` feature:

```toml
[dependencies.lspower]
version = "*"
default-features = false
features = ["runtime-agnostic"]
```

## License

`lspower` is free and open source software distributed under either the
[MIT](LICENSE-MIT) or the [Apache 2.0](LICENSE-APACHE) license, at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

## Acknowledgements

`lspower` is a fork of the [`tower-lsp`](https://github.com/ebkalderon/tower-lsp) crate.
