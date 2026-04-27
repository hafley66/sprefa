//! sprefa server — daemon shape per `chat_log/20260427.1.v3-server-port-zoom3.md`.
//!
//! Topology:
//!   - `sprefa-server` (long-lived daemon): owns `ServerState`. Listens
//!     on unix socket + TCP. Writes `server.json` for shells to find.
//!   - `sprefa-lsp` (~100 LoC stdio↔WS proxy): editor spawns it, it
//!     dials `server.json` `/lsp` endpoint.
//!   - `sprefa-run` (HTTP client): POST `/run`, stream SSE events.
//!
//! `Backend::new(client)` still works for legacy stdio mode (one
//! private ServerState); `Backend::with_state(client, state)` is the
//! daemon mode that shares one ServerState across every WS connection.

pub mod backend;
pub mod config;
pub mod position;
pub mod run;
pub mod session;
pub mod state;
pub mod transport_http;
pub mod transport_lsp;
pub mod workspace;

pub use backend::Backend;
pub use config::{Config, Seed};
pub use run::{run_pipeline, RunEvent, RunRequest, RunStart};
pub use session::DocSession;
pub use state::{
    default_info_path, default_log_path, default_unix_socket_path, HttpInfo,
    ServerInfo, ServerOpts, ServerState,
};
pub use transport_http::{serve_http, HttpOpts};
