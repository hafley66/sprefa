// sprefa-engine-rs: the RumbleRuntime the emit_rust emitter lowers against.
//
// The crate's header module is types.ts's Rust analogue: every public type is
// declared here (or in the typed modules it re-exports) rather than as bare
// free functions, satisfying the v6 interface-declaration law by construction.

pub mod driver;
pub mod incremental;
pub mod program;
pub mod sql;
pub mod text_plane;
pub mod ticklog;
pub mod types;

pub use program::GenProgram;
pub use sql::{result_rows, SqlRunner, SqliteSeam};
pub use ticklog::{js_float_text, tick_line};
pub use types::*;
