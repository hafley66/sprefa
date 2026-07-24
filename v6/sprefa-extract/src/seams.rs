//! S5 CPU trait seams. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::seams::*` import paths keep resolving.
pub use crate::types::{BlobSource, ParseError, Parser, Project};
