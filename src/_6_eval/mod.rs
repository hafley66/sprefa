//! The stratified semi-naive Datalog evaluator. Port of
//! `v7/src/1_libtime/0_evaluator.pl` `evaluate/4`; the oracle fixtures under
//! `v8/oracle/eval/` are frozen v7 output.

#[path = "_9_dd_eval.rs"]
pub mod dd_eval;
#[path = "_5_evaluate.rs"]
pub mod evaluate;
#[path = "_8_functions.rs"]
pub mod functions;
#[path = "_9c_ir_bundle.rs"]
pub mod ir_bundle;
#[path = "_9b_ir_eval.rs"]
pub mod ir_eval;
#[path = "_9a_ir_lower.rs"]
pub mod ir_lower;
#[path = "_6_json.rs"]
pub mod json;
#[path = "_4_kernel.rs"]
pub mod kernel;
#[path = "_4a_str_ops.rs"]
pub mod str_ops;
#[path = "_1_program.rs"]
pub mod program;
#[path = "_7_sqlite_eval.rs"]
pub mod sqlite_eval;
#[path = "_2_stratify.rs"]
pub mod stratify;
#[path = "_3_table.rs"]
pub mod table;
#[path = "_0_term.rs"]
pub mod term;

pub use dd_eval::evaluate_dd;
pub use evaluate::{evaluate, Closure, Trace};
pub use program::{Arg, Diagnostic, Goal, Polarity, Program, Row, Rule};
pub use sqlite_eval::{evaluate_sqlite, IEvaluate, SeedDelta, SqliteEvaluate};
pub use term::{Sym, Term, TermId, Universe};
