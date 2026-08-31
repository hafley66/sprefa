//! S2 family model. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::family::*` import paths keep resolving.
//! `ProjectEdge<F>` (the per-family resolved edge row) rides this shim because
//! `rows.rs` is a frozen seam this increment (commit 4a).
//!
//! ── THE FAMILY VOCABULARY, AND WHAT "DIET" MEANS ────────────────────────────
//!
//! DIET MEANS PARSE TECHNIQUE AND HEURISTICS, NEVER ACTUAL SCIP DATA.
//!
//! Two `--family` names on the CLI are whole-project MODES rather than members
//! of the per-file mask below, and the split between them is the point:
//!
//!   `scip`       REAL SCIP index data. A language's own indexer (rust-analyzer,
//!                scip-typescript, scip-go) runs over the root under a budget,
//!                its index.scip is decoded, and the rows are v5's `scip_*`
//!                relation shapes. Every fact is compiler-resolved.
//!
//!   `diet_scip`  This crate's OWN tree-sitter / oxc / syn front-ends plus
//!                name-match resolution across the supplied file set. No
//!                indexer, no type checker, no index. It is fast and needs no
//!                toolchain, and it is silently wrong wherever a name is
//!                ambiguous corpus-wide: two files defining `helper` make every
//!                unqualified call to `helper` unresolvable, where a real index
//!                resolves it through the import.
//!
//! The four names below (`cst`, `type`, `call`, `df`) are unaffected: they are
//! the per-file extraction mask and every existing caller of them keeps working
//! exactly as before. `--resolve` likewise remains the pre-existing spelling of
//! the pass `diet_scip` labels.

pub use crate::types::{
    flow_edges, CallEdgeKind, CallF, CallFAux, CallKind, CallSite, ConstKind, ConstValue,
    CstEdgeKind, CstF, DfArg, DfEdgeKind, DfF, DfFAux, DfField, DfLit, DfNodeKind, DfParam,
    DocFact, DocTag, Family, FlowEdge, FlowEdgeKind, FlowF, MethodOwner, ProjectEdge, PyBind,
    PyCallArg, PyDecor, PyDefault, PyParam, PyRetCall, PyReturn, PySubCall, ReceiverBinding,
    ReceiverOutcome, RefPosition, Reference, SigSlot, Specifier, SpecifierKind, TypeEdgeCandidate,
    TypeEdgeKind, TypeEntityKind, TypeF, TypeFAux, TypeSig,
};
