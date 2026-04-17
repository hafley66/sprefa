//! sprefa-server runtime.
//!
//! Long-lived HTTP server. Owns tokio runtime, SQLite store, shared
//! ResultStore, LSP layer, workspaces, watcher (later), auto-run rules (later).
//! LSP and CLI are thin shells over this.
//!
//! D1 slice: types + stubs. Existing bin/sprefa_v2_lsp keeps working
//! independently until D3 wires LspAdapter onto ServerState.

pub mod _0_state;
pub mod _1_workspace;
pub mod _2_lsp_layer;
pub mod _3_run;
pub mod _4_transport_lsp;
pub mod _5_transport_http;

pub use _0_state::{
    default_info_path, default_log_path, default_store_path, default_unix_socket_path,
    HandlerProto, HttpInfo, ServerInfo, ServerOpts, ServerState,
};
pub use _1_workspace::{WorkspaceCtx, WorkspaceRegistry};
pub use _2_lsp_layer::{LspLayer, LspOutbound, LspSession};
pub use _3_run::{run_pipeline, RunRequest, RunStart};
pub use _5_transport_http::{serve_http, HttpOpts, StatusDto};
