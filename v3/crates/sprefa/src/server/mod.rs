//! sprefa-server runtime.
//!
//! Long-lived HTTP server. Owns tokio runtime, SQLite store, shared
//! ResultStore, LSP layer, workspaces, watcher (later), auto-run rules (later).
//! LSP and CLI are thin shells over this.
//!
//! D1 slice: types + stubs. Existing bin/sprefa_v2_lsp keeps working
//! independently until D3 wires LspAdapter onto ServerState.

pub mod state;
pub mod workspace;
pub mod lsp_layer;
pub mod run;
pub mod transport_lsp;
pub mod transport_http;

pub use state::{
    default_info_path, default_log_path, default_store_path, default_unix_socket_path,
    HandlerProto, HttpInfo, ServerInfo, ServerOpts, ServerState,
};
pub use workspace::{WorkspaceCtx, WorkspaceRegistry};
pub use lsp_layer::{LspLayer, LspOutbound, LspSession};
pub use run::{run_pipeline, RunRequest, RunStart};
pub use transport_http::{serve_http, HttpOpts, StatusDto};
