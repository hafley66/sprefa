// sprefa-engine-rs: the RumbleRuntime the emit_rust emitter lowers against.
//
// The crate's header module is types.ts's Rust analogue: every public type is
// declared here (or in the typed modules it re-exports) rather than as bare
// free functions, satisfying the v6 interface-declaration law by construction.

pub mod change_facts;
pub mod driver;
pub mod enum_plane;
pub mod executors;
pub mod hosts;
pub mod incremental;
pub mod program;
pub mod run;
pub mod serve;
pub mod source_bind;
pub mod sql;
pub mod struct_plane;
pub mod text_plane;
pub mod ticklog;
pub mod trace;
pub mod types;
pub mod write_verbs;

pub use program::GenProgram;
pub use run::{
    run_once, watch, FinalRel, FinalRequest, LoadedProgram, RunOptions, RunOutcome, SeedSpec,
    WatchOptions,
};
pub use serve::{router, serve_on, serve_unix, ServeError, ServeState};
pub use sql::{result_rows, SqlRunner, SqliteSeam};
pub use ticklog::{js_float_text, tick_line};
pub use trace::Scope as TraceScope;
pub use types::*;
