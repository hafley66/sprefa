//! The demand cone AT THE EXTRACTION LAYER, and the projection bundles.
//!
//! v5 already had `AnalysisMask { types, calls, dataflow }` and `AnalysisBundle`
//! (`typegraph/mod.rs:469-485`) — extract only the families asked for, sharing
//! one parse. v6 (a) extends the mask with `module` (the delivery-plan gap), and
//! (b) makes it the EXTRACTION-SIDE twin of the engine's subscription cone: a
//! subscription activates a family cone, and a cold blob extracts ONLY that cone.
//! Combined with content-addressing, an unchanged blob + an unchanged mask is a
//! cache hit at zero parse cost (v5's per-rev digest skip, generalized).
//!
//! The bundle is split along the two-phase seam (`_2_traits`): a `FileBundle`
//! (phase 1, content-addressed) holds nodes + intra-file edges; a `ProjectBundle`
//! (phase 2, project-scoped) holds the cross-file `ProjectEdge`s + the binding
//! side table. A family that needs the file set to resolve (module/type/call)
//! appears in both; a content-only family (df) appears only in the FileBundle.

use crate::_3_extract::_0_shape::{BlobHash, NameId};
use crate::_3_extract::_3_facts::{
    CallFileFacts, DfFileFacts, ModuleFileFacts, TypeFileFacts,
};
use crate::_3_extract::_3_facts::{CallProjectFacts, ModuleProjectFacts, TypeProjectFacts};

/// Which families to extract. Bitflags-shaped (a plain struct of bools; cheap
/// to copy, easy to partition the cache by). The engine passes the active cone;
/// extract returns only the `Some(_)` arms of the bundle.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FamilyMask {
    pub df: bool,
    pub call: bool,
    pub type_: bool,
    pub module: bool,
}

impl FamilyMask {
    pub const NONE: Self = Self { df: false, call: false, type_: false, module: false };
    pub const ALL: Self = Self { df: true, call: true, type_: true, module: true };
    /// df is the only family with NO project phase (no cross-file resolution).
    pub fn needs_project(self) -> bool {
        self.call || self.type_ || self.module
    }
}

/// Phase-1 output: one blob's worth of nodes + intra-file edges, for the masked
/// families. Cache key `(BlobHash, lang, FamilyMask)`. The engine interns these
/// into `node`/`edge`; an unchanged blob + mask never re-parses.
#[derive(Clone, Debug, Default)]
pub struct FileBundle {
    pub mask: FamilyMask,
    pub df: Option<DfFileFacts>,
    pub call: Option<CallFileFacts>,
    pub type_: Option<TypeFileFacts>,
    pub module: Option<ModuleFileFacts>,
}

/// Phase-2 output: the cross-file resolutions for one blob, given the file set.
/// Cache key `(BlobHash, project_digest, FamilyMask)`. `project_digest` changes
/// when any file appears/disappears (a changed resolution); v5's `ProjectCx`
/// OnceLock indexes feed this. df contributes nothing here.
#[derive(Clone, Debug, Default)]
pub struct ProjectBundle {
    pub mask: FamilyMask,
    pub call: Option<CallProjectFacts>,
    pub type_: Option<TypeProjectFacts>,
    pub module: Option<ModuleProjectFacts>,
}

/// The per-name binding side table (v5 `module_binding`), shared by the module
/// family across both phases: a local name -> imported name + how it entered
/// scope. Not a node and not an edge; a family side table (delivery-plan ruling).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub local: NameId,
    pub source: NameId,        // the module specifier the name came from
    pub imported: NameId,      // the name as written in the source module
    pub kind: crate::_3_extract::_0_shape::BindingKind,
}

/// A digest of the file set that affects resolution (which files exist + their
/// manifest membership). Folded from the corpus so two identical blobs in
/// identical file-set contexts share phase-2 work.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProjectDigest(pub [u8; 16]);

/// Marker: the blob the phase-2 resolver is looking at. Held so `ProjectCx` can
/// stay a borrowed view while the per-blob resolution carries its own content key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BlobId(pub BlobHash);
