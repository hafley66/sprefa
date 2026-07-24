//! S5 CPU trait seams. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::seams::*` import paths keep resolving.
//! Commit 4a adds the phase-2 seam: `Resolve` + the `ProjectCx` IO view with
//! its hollow satellites (FileSet / ManifestMap / IndexBag / ProjectDigest) —
//! the same placement as the seed's `_2_traits.rs`, which holds them beside the
//! phase-1/phase-2 traits.
pub use crate::types::{
    BlobSource, FileSet, IndexBag, ManifestMap, ParseError, Parser, Project, ProjectCx,
    ProjectDigest, Resolve,
};
