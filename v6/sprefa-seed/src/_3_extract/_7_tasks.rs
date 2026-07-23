//! THE PARITY SURFACE — the extraction contract as a living task ledger.
//!
//! Mirrors `v6/sprefa-store/src/tasks.rs`: the traits are the contract, the
//! `Tasks` impl is a stub whose method bodies are `todo!()` and whose DOCS are
//! the task notes (oracle / built? / parity test / status), and `ExtractPlan`
//! is the proof-token epic ledger. Read the plan off this file.
//!
//! ── the mind (one crate, one job) ────────────────────────────────────────────
//! Turn source bytes into the normalized node/edge shape the store owns
//! (`_0_shape`). Pure, SYNC, CPU-bound, rayon-parallel, arena-mastered. Content
//! in, `RawNode`/`RawEdge` out, NO database, NO async, NO reactor. The async-eval
//! flip is a LATER layer's problem (engine/server); this crate draws the sync
//! boundary at the engine seam, where the crate-map sync/async ruling puts the
//! tick hot path. A fixpoint has nothing to await and neither does a parse.
//!
//! ── THE NORMALIZATION (what v6 deletes from v5, structurally) ────────────────
//!   v5 had FOUR span shapes, THREE `kind` reps, and SPLIT node identity
//!   (`mint_sym` strings / dense `NodeIdx` / salted `WhereBytes`). v6 has ONE
//!   `Span`, ONE typed-kind ordinal per family, node id = (family, span, kind).
//!   The `mint_sym`/`fn_sym` disease (~63% of v5's dictionary, the resident-heap
//!   amplifier) is impossible by construction: a fact stores a span + a `NameId`,
//!   never a qualified string.
//!
//! ── THE TWO-PHASE SPLIT (the ONE distinction the unification PRESERVES) ─────
//!   FileExtract    = pure fn of content         key (blob, lang, mask)         -> nodes
//!   ProjectExtract = pure fn of content+fileset key (blob, project_digest, mask) -> edges
//!   Stack Graphs calls this index vs query; Kythe calls it anchor vs semantic;
//!   SCIP calls it Occurrence.range vs Symbol. Forcing one trait would erase the
//!   content-addressed dedup (a file byte-identical across 50 revs extracts ONCE)
//!   that is the whole point.
//!
//! ── THE TIER MODEL (formalizes v5's implicit Tier 1/2 + floor) ───────────────
//!   Tier 1 SCIP      — compiler-backed, shelled out, ground truth for resolution
//!   Tier 2 native AST— syn / oxc / tree-sitter; owns dataflow (SCIP has no CFG)
//!   floor tree-sitter— the universal backup + the CST family
//!   Per-language `Source` binds the three; the dispatcher MERGES (SCIP overrides
//!   for call/type/module; AST fills df + spans; floor covers the rest).
//!
//! ── the RAM discipline (the v5 36GB-swap death, killed at the type level) ───
//!   ARENA-PER-FILE on rayon: each worker owns a `ParseArena`, parses one file,
//!   projects to owned `RawNode`/`RawEdge` Vecs, drops the arena. The CST never
//!   crosses a thread; peak RSS = biggest single file, NOT the corpus.
//!
//! Companion epic plan: `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`.

use crate::_3_extract::_0_shape::{ProjectEdge, RawEdge, RawNode};
use crate::_3_extract::_1_mask::{FamilyMask, FileBundle, ProjectBundle};
use crate::_3_extract::_2_traits::{ExtractBudget, ExtractJob, ProjectCx, Source};
use crate::_3_extract::_4_scip::{ScipError, ScipIndex};

// Proof tokens RELEASED by an unlanded task (mirrors store tasks.rs convention):
//   Arened       epic 2 : arena-per-file RSS stays flat under N-worker parse
//   Merged       epic 3 : SCIP+AST tier merge yields byte-identical resolution
//   Dispatched   epic 4 : rayon dispatch hits no lock contention / livelock
//   FlowUnified  epic 5 : flow_edge (the v5 stdlib 5th family) promoted to typed edges
//   Evidence     frontier: a measurement that would close a question
pub struct Arened;
pub struct Merged;
pub struct Dispatched;
pub struct FlowUnified;
pub struct Evidence;

// =============================================================================
// Trait · Extract — the contract surface (the operations, each doc = the note)
// =============================================================================
/// The extraction contract. SYNC throughout. The engine calls `dispatch` with
/// the changed blobs + the active cone mask; extract returns normalized nodes +
/// edges + aux for exactly the masked families.
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

/// What one blob's extraction yields: phase-1 + phase-2 flattened to the
/// `_0_shape` rows the engine interns. `aux` carries the family side projections
/// (param_pos/args/fields/lits/loops/docs/consts/bindings).
#[derive(Clone, Debug, Default)]
pub struct ExtractOutput {
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    pub project_edges: Vec<ProjectEdge>,
    pub aux: AuxFacts,
}

/// The family side tables (Joern "edge properties" done as side projections, per
/// the store's per-family side-table ruling). Opaque in the type math; each
/// field mirrors a v5 aux rel (df_param/df_arg/df_field/df_lit/loop_over/...).
#[derive(Clone, Debug, Default)]
pub struct AuxFacts;

/// SCIP resolution layered over AST facts. The engine writes SCIP-resolved edges
/// with precedence; AST-only edges fill the rest.
#[derive(Clone, Debug, Default)]
pub struct MergedBundle {
    pub ast: Vec<ExtractOutput>,
    pub scip_resolution: Vec<ProjectEdge>,
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
    fn arena_ram_mastered(&self, budget: &ExtractBudget) -> Arened;
    /// 3  SCIP + AST tier merge yields byte-identical resolution vs v5.
    fn tiers_merged(&self, proof: &Arened) -> Merged;
    /// 4  rayon dispatch over a real corpus hits no lock contention / livelock.
    fn parallel_dispatch_proven(&self, proof: &Merged) -> Dispatched;
    /// 5  flow_edge (v5 stdlib 5th family, std/flow.dl:89) promoted to typed
    ///    `FlowEdgeKind` edges — the interprocedural value-flow union in the type
    ///    system, not a stringly-joined .dl rel.
    fn flow_edge_promoted(&self, proof: &Dispatched) -> FlowUnified;
    /// frontier: does per-site callee cloning (k-CFA) + node-level types +
    ///    dominators/CFG (the v5-stated gaps) belong in extract or in the engine?
    ///    Returns Evidence, not a shipped change.
    fn intra_procedural_frontier(&self) -> Evidence;
}
