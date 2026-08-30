//! S5 CPU trait seams. Canonical definitions now live in `crate::types`; this
//! module is a re-export so `crate::seams::*` import paths keep resolving.
//! Commit 4a adds the phase-2 seam: `Resolve` + the `ProjectCx` IO view with
//! its hollow satellites (FileSet / ManifestMap / IndexBag / ProjectDigest) —
//! the same placement as the seed's `_2_traits.rs`, which holds them beside the
//! phase-1/phase-2 traits. The ADDENDUM adds the shared resolve machinery:
//! `DefIndex` / `DefSite` / `build_def_index` + the pure helper fns
//! (`covering_def` / `def_named` / `corpus_defs`) every lang resolve arm uses.
//! Commit 4c-i adds the Tier-1 seam: `ScipSource` + the diet wire types
//! (`ScipIndex` / `ScipDocument` / `ScipOccurrence` / `ScipSymbolInfo` /
//! `OccurrenceRole` / `PositionEncoding` / `ScipError`); the build/load logic
//! lives in `crate::scip`.
pub use crate::types::{
    build_def_index, containing_def_site, containing_def_site_in, corpus_defs, covering_def,
    def_named, own_blob, BlobSource, DefIndex, DefSite, FileSet, IndexBag, ManifestMap,
    OccurrenceRole, ParseError, Parser, PositionEncoding, Project, ProjectCx, ProjectDigest,
    Resolve, ScipDiagnostic, ScipDocument, ScipError, ScipIndex, ScipMetadata, ScipOccurrence,
    ScipRelationship, ScipSignature, ScipSource, ScipSymbolInfo,
};
