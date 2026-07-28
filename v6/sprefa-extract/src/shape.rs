//! S1 atoms. Canonical definitions now live in `crate::types`; this module is a
//! re-export so `crate::shape::*` import paths keep resolving.
pub use crate::types::{BlobHash, FamilyTag, NameId, NodeRef, Span, Strings};
