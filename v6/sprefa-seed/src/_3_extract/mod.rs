//! sprefa-extract — THE MIND. The parity surface as a living task ledger.
//!
//! Role (one crate, one job): turn source bytes into the normalized node/edge
//! shape the store owns (`_0_shape`). Pure, SYNC, CPU-bound, rayon-parallel,
//! arena-mastered. Content in, `RawNode`/`RawEdge` out, NO database, NO async,
//! NO reactor. The async-eval flip is a LATER layer's problem (engine/server);
//! this crate draws the sync boundary at the engine seam, exactly where the
//! crate-map sync/async ruling puts the tick hot path. A fixpoint has nothing
//! to await and neither does a parse.
//!
//! THE NORMALIZATION (what v6 deletes from v5, structurally — not patched):
//!   v5 had FOUR span shapes, THREE `kind` representations, and SPLIT node
//!   identity (`mint_sym` coordinate strings / dense NodeIdx / salted WhereBytes).
//!   v6 has ONE span (`Span`), ONE typed-kind ordinal per family (`_0_shape`),
//!   and node identity = (family, span, kind). The `mint_sym`/`fn_sym`/`salt_rev`
//!   disease (~63% of v5's dictionary, the resident-heap amplifier) is impossible
//!   by construction: a fact stores a span + a NameId, never a qualified string.
//!
//! THE TWO-PHASE SPLIT (the ONE distinction the unification preserves):
//!   FileExtract    = pure fn of content         key (blob, lang, mask)        -> nodes
//!   ProjectExtract = pure fn of content+fileset key (blob, project_digest, mask) -> edges
//!   Stack Graphs calls this index-phase vs query-phase; Kythe calls it anchor
//!   vs semantic; SCIP calls it Occurrence.range vs Symbol. Same split, three
//!   names. Forcing one trait would erase the content-addressed dedup (a file
//!   byte-identical across 50 revs extracts ONCE) that is the whole point.
//!
//! THE TIER MODEL (formalizes v5's implicit Tier 1/2 + floor):
//!   Tier 1 SCIP      — compiler-backed, shelled out, ground truth for resolution
//!   Tier 2 native AST— syn / oxc / tree-sitter; owns dataflow (SCIP has no CFG)
//!   floor tree-sitter— the universal backup + the CST family
//!   Per-language `Source` binds the three; the dispatcher MERGES (SCIP overrides
//!   for call/type/module; AST fills df + spans; floor covers the rest).
//!
//! "Useless" traits: declared, NOT wired. Each impl body is `todo!()` and its
//! doc IS the task note (oracle, built?, parity test, status) — read the plan
//! off this file. When a task lands, the real impl (in the real crate) replaces
//! the note. The companion epic plan is
//! `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`.
#![allow(dead_code, unused_imports, clippy::new_without_default)]

pub mod _0_shape;
pub mod _1_mask;
pub mod _2_traits;
pub mod _3_facts;
pub mod _4_scip;
pub mod _5_term;

use crate::_3_extract::_0_shape::{NodeRef, RawEdge, RawNode};
use crate::_3_extract::_1_mask::{FamilyMask, FileBundle, ProjectBundle};
use crate::_3_extract::_2_traits::{
    ExtractBudget, ExtractJob, FileCacheKey, ProjectCx, Source,
};
use crate::_3_extract::_4_scip::{ScipError, ScipIndex};

// Real types the plan consumes (resolved when the crate is real):
//   RawNode / RawEdge   = crate::_0_shape   — what crosses extract -> engine
//   Span / NameId       = crate::_0_shape   — the ONE coordinate + the interner
//   ScipSource          = crate::_4_scip    — the shell-out Tier-1 seam
// Mechanism = ARENA-PER-FILE on rayon: each worker owns a ParseArena, parses one
// file, projects to owned RawNode/RawEdge Vecs, drops the arena. The CST never
// crosses a thread; peak RSS = biggest single file, NOT the corpus. This is the
// oxc/biome pattern and the only shape that keeps RAM flat under N parallel
// workers — the v5 36GB-swap death made impossible.

// Proof tokens RELEASED by an unlanded task (mirrors store tasks.rs convention):
pub struct Arened;          // epic 2 : arena-per-file RSS stays flat under N-worker parse
pub struct Merged;          // epic 3 : SCIP+AST tier merge yields byte-identical resolution
pub struct Dispatched;      // epic 4 : rayon dispatch hits no lock contention / livelock
pub struct FlowUnified;     // epic 5 : flow_edge (the v5 stdlib 5th family) promoted to typed edges
pub struct Evidence;        // frontier: a measurement that would close a question

// =============================================================================
// Trait · Extract — the contract surface (the operations, each doc = the note)
// =============================================================================
/// The extraction contract. SYNC throughout. The engine calls `dispatch` with
/// the changed blobs + the active cone mask; extract returns normalized nodes +
/// edges + aux for exactly the masked families. Every method's doc states its
/// oracle and status — this is the living ledger.
pub trait Extract {
    /// 🧪 OPEN · rayon fan-out over `jobs`, one arena per worker · oracle: v5
    /// extractors (syn/oxc/tree-sitter) byte-identical on the same corpus ·
    /// parity: byte-identical RawNode/RawEdge set vs v5 · THE RAM GUN (RSS flat).
    fn dispatch(
        &self,
        jobs: Vec<ExtractJob>,
        cx: &ProjectCx,
        sources: &[Source],
        budget: &ExtractBudget,
    ) -> Vec<ExtractOutput>;

    /// 🧪 OPEN · phase 1: one parse, masked projections · cache key (blob, lang,
    /// mask) · oracle v5 `TypeLang::extract_bundle` · identical bytes = one hit.
    fn extract_file(&self, job: &ExtractJob, sources: &[Source]) -> FileBundle;

    /// 🧪 OPEN · phase 2: cross-file resolution · cache key (blob, project_digest,
    /// mask) · oracle v5 `ModuleResolver::edges` + type/call resolvers.
    fn resolve_project(
        &self,
        blob: &ExtractJob,
        file: &FileBundle,
        cx: &ProjectCx,
        sources: &[Source],
    ) -> ProjectBundle;

    /// 🧪 OPEN · Tier 1: shell out the foreign indexer over `root` · oracle v5
    /// `scip_setup` INDEXERS · subprocess/IPC, never bespoke FFI.
    fn scip_build(&self, root: &std::path::Path, indexer: &'static str) -> Result<(), ScipError>;

    /// 🧪 OPEN · Tier 1: parse index.scip -> diet ScipIndex · oracle v5
    /// `scip_import::load` · reload-gated by mtime.
    fn scip_load(&self, index_path: &std::path::Path) -> Result<ScipIndex, ScipError>;

    /// 🧪 OPEN · tier merge: SCIP def/ref = ground truth for call/type/module
    /// resolution; AST fills dataflow + spans · oracle: SCIP-vs-AST agreement
    /// on shared families · releases `Merged`.
    fn merge(&self, scip: &ScipIndex, ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle;
}

/// What one blob's extraction yields: the phase-1 + phase-2 bundles, flattened
/// to the `_0_shape` rows the engine interns. aux carries the family side
/// projections (param_pos/args/fields/lits/loops/docs/consts/bindings).
#[derive(Clone, Debug, Default)]
pub struct ExtractOutput {
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    pub project_edges: Vec<crate::_3_extract::_0_shape::ProjectEdge>,
    pub aux: AuxFacts,
}

/// The family side tables (Joern "edge properties" done as side projections, per
/// the store's per-family side-table ruling). Opaque in the type math; each
/// field mirrors a v5 aux rel (df_param/df_arg/df_field/df_lit/loop_over/...).
#[derive(Clone, Debug, Default)]
pub struct AuxFacts;

/// The merged bundle: SCIP resolution layered over AST facts. The engine writes
/// SCIP-resolved edges with precedence; AST-only edges fill the rest.
#[derive(Clone, Debug, Default)]
pub struct MergedBundle {
    pub ast: Vec<ExtractOutput>,
    pub scip_resolution: Vec<crate::_3_extract::_0_shape::ProjectEdge>,
}

// =============================================================================
// The stub impl — every body is `todo!()`; the doc on each method IS the note.
// =============================================================================
pub struct Tasks;

impl Extract for Tasks {
    /// 🧪 OPEN · rayon dispatch, arena-per-file · oracle v5 · RAM gun.
    fn dispatch(
        &self,
        _jobs: Vec<ExtractJob>,
        _cx: &ProjectCx,
        _sources: &[Source],
        _budget: &ExtractBudget,
    ) -> Vec<ExtractOutput> {
        todo!("epic 4: rayon par_iter over jobs; each worker owns ParseArena; budget-cap RSS")
    }
    /// 🧪 OPEN · phase 1 · cache key (blob, lang, mask).
    fn extract_file(&self, _job: &ExtractJob, _sources: &[Source]) -> FileBundle {
        todo!("epic 1: tiered parse -> FileExtract::extract(path, bytes, mask)")
    }
    /// 🧪 OPEN · phase 2 · cache key (blob, project_digest, mask).
    fn resolve_project(
        &self,
        _blob: &ExtractJob,
        _file: &FileBundle,
        _cx: &ProjectCx,
        _sources: &[Source],
    ) -> ProjectBundle {
        todo!("epic 1: ProjectExtract::resolve for call/type/module; df skipped")
    }
    /// 🧪 OPEN · Tier 1 build (subprocess).
    fn scip_build(&self, _root: &std::path::Path, _indexer: &'static str) -> Result<(), ScipError> {
        todo!("epic 3: shell out rust-analyzer/scip-typescript/...; write index.scip")
    }
    /// 🧪 OPEN · Tier 1 load (protobuf parse).
    fn scip_load(&self, _index_path: &std::path::Path) -> Result<ScipIndex, ScipError> {
        todo!("epic 3: parse index.scip -> diet ScipIndex (symbol/range/role/relations only)")
    }
    /// 🧪 OPEN · tier merge · SCIP overrides resolution; AST fills df + spans.
    fn merge(&self, _scip: &ScipIndex, _ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle {
        todo!("epic 3: SCIP ground-truth for call/type/module; AST for df; releases Merged")
    }
}

// =============================================================================
// The remaining plan, as a trait — proof-token methods for the open epics.
// =============================================================================
/// A method's ARGS are body predicates (facts released earlier); its RETURN is
/// the head predicate. Epic 2 masters RAM; epic 3 merges tiers; epic 4 proves
/// parallelism; epic 5 promotes flow_edge; the frontier measures the rest.
pub trait ExtractPlan {
    /// 2  arena-per-file parse keeps RSS flat under N-worker rayon dispatch.
    ///    The lab stresses arena_bytes + rss_bytes to the breaking point.
    fn arena_ram_mastered(&self, budget: &ExtractBudget) -> Arened;
    /// 3  SCIP + AST tier merge yields byte-identical resolution vs v5.
    fn tiers_merged(&self, proof: &Arened) -> Merged;
    /// 4  rayon dispatch over a real corpus hits no lock contention / livelock.
    fn parallel_dispatch_proven(&self, proof: &Merged) -> Dispatched;
    /// 5  flow_edge (v5 stdlib 5th family, std/flow.dl:89) promoted to typed
    ///    FlowEdgeKind edges — the interprocedural value-flow union in the type
    ///    system, not a stringly-joined .dl rel.
    fn flow_edge_promoted(&self, proof: &Dispatched) -> FlowUnified;
    /// frontier: does per-site callee cloning (k-CFA) + node-level types +
    ///    dominators/CFG (the v5-stated gaps) belong in extract or in the engine?
    ///    Returns Evidence, not a shipped change.
    fn intra_procedural_frontier(&self) -> Evidence;
}
