//! Datalog evaluation through `ivm_engine::Engine`; the oracle fixtures under
//! `oracle/eval/` are frozen v7 output.

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
#[path = "_2_stratify.rs"]
pub mod stratify;
#[path = "_3_table.rs"]
pub mod table;
#[path = "_0_term.rs"]
pub mod term;

pub use evaluate::{evaluate, Closure, Trace};
pub use program::{Arg, Diagnostic, Goal, Polarity, Program, Row, Rule};
pub use term::{Blob, Sym, Term, TermId, Universe};
