//! sprefa-lsp — stdio binary fitting the Backend on tokio stdin/stdout.
//!
//! Launch via VS Code's generic LSP extension (or any LSP client) by
//! pointing `server.command` at this binary. Tracing logs go to stderr;
//! control with `RUST_LOG` (e.g. `RUST_LOG=sprefa=debug`).

use server::Backend;
use tower_lsp::{LspService, Server};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        // VS Code's LSP output channel doesn't render ANSI; force-disable
        // colour so the log stays readable in the channel view.
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
