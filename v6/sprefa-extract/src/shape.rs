//! S1 atoms. Canonical definitions now live in `crate::types`; this module is a
//! re-export so `crate::shape::*` import paths keep resolving.
pub use crate::types::{
    content_id_of, ContentId, FamilyTag, NameId, NodeRef, Span, Strings, ZERO_CONTENT_ID,
};
