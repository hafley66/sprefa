//! S2 family model. Canonical definitions now live in `crate::types`; this module
//! is a re-export so `crate::family::*` import paths keep resolving.
//! `ProjectEdge<F>` (the per-family resolved edge row) rides this shim because
//! `rows.rs` is a frozen seam this increment (commit 4a).
pub use crate::types::{
    CallEdgeKind, CallF, CallFAux, CallKind, CallSite, ConstKind, ConstValue, CstEdgeKind, CstF,
    DfEdgeKind, DfF, DfNodeKind, Family, ProjectEdge, SigSlot, Specifier, SpecifierKind,
    TypeEdgeCandidate, TypeEdgeKind, TypeEntityKind, TypeFAux, TypeF, TypeSig,
};
