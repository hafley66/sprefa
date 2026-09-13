//! The stratified semi-naive Datalog evaluator. Port of
//! `v7/src/1_libtime/0_evaluator.pl` `evaluate/4`; the oracle fixtures under
//! `v8/oracle/eval/` are frozen v7 output.

#[path = "_5_evaluate.rs"]
pub mod evaluate;
#[path = "_6_json.rs"]
pub mod json;
#[path = "_4_kernel.rs"]
pub mod kernel;
#[path = "_1_program.rs"]
pub mod program;
#[path = "_2_stratify.rs"]
pub mod stratify;
#[path = "_3_table.rs"]
pub mod table;
#[path = "_0_term.rs"]
pub mod term;

pub use evaluate::{evaluate, Closure, Trace};
pub use program::{Arg, Diagnostic, Goal, Polarity, Program, Row, Rule};
pub use term::{Sym, Term, TermId, Universe};
