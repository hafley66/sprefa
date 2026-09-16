//! The shapes three v7 modules pass between each other, plus the one stop the
//! port needs where v7 simply fails.

// `compiled_unit/3` at `1_artifact_emitter.pl:23` is `2_compiler.pl:910`'s
// term; `crate::_4_comptime::Compiled` is the one struct for it.
use crate::_6_eval::term::TermId;

/// Where v7 fails instead of returning a diagnostic, and where an arm is not
/// built yet. Never a behaviour change: every variant is a shape v7's clause
/// heads reject by failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Fail(&'static str),
}

/// `compiler_view(TypeGraphFacts, CompilerFacts, LogicalProgramRows,
/// RuntimeProgram)` at `1_artifact_emitter.pl:24-25`.
pub struct CompilerView {
    pub type_graph_facts: Vec<TermId>,
    pub compiler_facts: Vec<TermId>,
    pub logical_program_rows: Vec<TermId>,
    pub runtime_program: TermId,
}

/// `logical_program_calls/4,5` and `logical_program_rows_calls/5` outputs.
/// Calls keep row order (`0_logical_program_reifier.pl:67`); diagnostics are
/// sorted (`:68`).
pub struct Calls {
    pub calls: Vec<TermId>,
    pub diagnostics: Vec<TermId>,
}

/// `emit_compiled/4` outputs at `1_artifact_emitter.pl:34`.
pub struct Emitted {
    pub artifact: TermId,
    pub diagnostics: Vec<TermId>,
}
