//! Macrotime: reader forms to a syntax graph, the `<+` rewrite waves evaluated
//! by `_6_eval`, and the graph back to reader forms. Parity with v7
//! `reify_syntax/4`, `expand_syntax/5` and `materialize_syntax/4`; the oracle
//! cases under `v8/oracle/macrotime/` are frozen v7 output.

#[path = "_0_rows.rs"]
pub mod _0_rows;
#[path = "_1_reify.rs"]
pub mod _1_reify;
#[path = "_2_protocol.rs"]
pub mod _2_protocol;
#[path = "_3_rewrite.rs"]
pub mod _3_rewrite;
#[path = "_4_expand.rs"]
pub mod _4_expand;
#[path = "_5_materialize.rs"]
pub mod _5_materialize;
#[path = "_6_cli.rs"]
pub mod _6_cli;

pub use _0_rows::Graph;
pub use _1_reify::reify;
pub use _2_protocol::{MacroProgram, Protocol};
pub use _4_expand::{expand, Wave, WAVE_LIMIT};
pub use _5_materialize::materialize;
pub use _6_cli::{cli, expansion_json, macro_program_from_json, run, Expansion};
