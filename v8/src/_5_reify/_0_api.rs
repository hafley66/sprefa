//! The shapes three v7 modules pass between each other, plus the one stop the
//! port needs where v7 simply fails.

use crate::_6_eval::term::TermId;

/// Where v7 fails instead of returning a diagnostic, and where an arm is not
/// built yet. Never a behaviour change: every variant is a shape v7's clause
/// heads reject by failing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Fail(&'static str),
}

/// `compiled_unit(TypeGraphFacts, RuntimeProgram, CompilerFacts)`
/// at `1_artifact_emitter.pl:23`, one field per functor argument.
///
/// `_4_comptime` is porting the same term as `Compiled` on a parallel branch.
/// The coordinator merges the two after both land; nothing outside
/// `_5_reify::emit` reads this struct.
pub struct CompiledUnit {
    pub type_graph_facts: Vec<TermId>,
    pub runtime_program: TermId,
    pub compiler_facts: Vec<TermId>,
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
