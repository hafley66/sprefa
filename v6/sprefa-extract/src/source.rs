//! The uniform surface: one `Source` per language + the masked bundle it returns.
//!
//! The v5 `TypeLang` analog the seed planned (`_7_tasks.rs:127`). v6 has uniform
//! leaves (`Parser`, `Project<F>`) but the orchestration above them was hand-rolled
//! per family (4 `dispatch_*` / 8 `flatten_*` / a hand-coded bin / 4 hand tests);
//! this collapses it. A new lang is ONE `Source` impl + one roster line + one
//! fixture (the turnkey contract, `_7_tasks.rs:158`).
//!
//! `Source::extract` owns its arena(s) internally and returns owned output: no
//! borrowed parse crosses the seam. Parse count is opaque to the trait (TS = 2
//! parses: ast-grep for cst + oxc for type/call/df; a masked projection is one
//! parse WITHIN an engine). A failed native parse leaves that engine's families
//! `None`; partial output survives (cst may be present when oxc fails). No panic.

use crate::family::{CallF, CstF, DfF, TypeF};
use crate::rows::FamilyBundle;
use crate::shape::Strings;

/// Which families to extract. One bool per family; the `Source` projects only the
/// masked ones (v5 `AnalysisMask{types,calls,dataflow}`, family-generic + concrete).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FamilyMask {
    pub cst: bool,
    pub types: bool,
    pub call: bool,
    pub df: bool,
}

impl FamilyMask {
    pub const ALL: Self = Self { cst: true, types: true, call: true, df: true };
    pub const NONE: Self = Self { cst: false, types: false, call: false, df: false };
}

/// One blob's extraction: the shared per-file interner + an `Option<FamilyBundle<F>>`
/// per family. Sharing ONE `Strings` (not one per family) is byte-stable: every
/// `flatten_*` resolves `NameId -> &str` at output, so the serialized JSONL is
/// identical to today's per-family interners (the dedup is the win, not a behavior
/// change). Fact identity unchanged: `(FamilyTag, Span, kind)`.
#[derive(Default)]
pub struct ExtractOutput {
    pub strings: Strings,
    pub cst: Option<FamilyBundle<CstF>>,
    pub types: Option<FamilyBundle<TypeF>>,
    pub call: Option<FamilyBundle<CallF>>,
    pub df: Option<FamilyBundle<DfF>>,
}

/// One language binding: a `Parser` + its per-family `Project<F>`s behind one
/// masked `extract`. The v6 `TypeLang` analog. Held `&'static` in the roster
/// (`lang::sources`); created once, no mutable state. `name`/`matches` mirror v5
/// `type_langs()` first-match roster (`typegraph/mod.rs:491`).
pub trait Source: Sync + Send {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// One parse per backing engine, masked projections. Owns the arena(s)
    /// internally; returns owned output (no borrowed parse crosses the seam).
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput;
}
