//! S5 CPU trait seams. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::seams::*` import paths keep resolving.
//! Commit 4a adds the phase-2 seam: `Resolve` + the `ProjectCx` IO view with
//! its hollow satellites (FileSet / ManifestMap / IndexBag / ProjectDigest) —
//! the same placement as the seed's `_2_traits.rs`, which holds them beside the
//! phase-1/phase-2 traits. The ADDENDUM adds the shared resolve machinery:
//! `DefIndex` / `DefSite` / `build_def_index` + the pure helper fns
//! (`covering_def` / `def_named` / `corpus_defs`) every lang resolve arm uses.
pub use crate::types::{
    BlobSource, DefIndex, DefSite, FileSet, IndexBag, ManifestMap, ParseError, Parser, Project,
    ProjectCx, ProjectDigest, Resolve, build_def_index, corpus_defs, covering_def, def_named,
};
