//! sprefa server — stdio LSP fitted on v3's pipeline + sprefa_parse.
//!
//! Stage B slice: parse-diagnostics only. Hover / completion are stubs
//! until DocSession grows past the parse layer.
//!
//! Shape:
//!   - `session::DocSession` owns one `.sprf` source buffer + latest
//!     ParsedSource + errors. Reparse on source-change.
//!   - `backend::Backend` is the tower-lsp `LanguageServer` impl. Per-
//!     URI DocSession map. did_open/did_change/did_save → publish
//!     diagnostics.
//!   - `position` converts byte offsets ↔ LSP line/col (UTF-16 code
//!     units per LSP spec).
//!
//! Binary entry point at `bin/sprefa-lsp.rs`.

pub mod backend;
pub mod config;
pub mod position;
pub mod session;

pub use backend::Backend;
pub use config::{Config, Seed};
pub use session::DocSession;
