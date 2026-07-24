//! S2 family model. Canonical definitions now live in `crate::types`; this module
//! is a re-export so `crate::family::*` import paths keep resolving.
pub use crate::types::{
    CallEdgeKind, CallF, CallFAux, CallKind, CallSite, ConstKind, ConstValue, CstEdgeKind, CstF,
    DfEdgeKind, DfF, DfNodeKind, Family, SigSlot, TypeEdgeKind, TypeEntityKind, TypeFAux, TypeF,
    TypeSig,
};
