//! THE PARITY SURFACE + CURRENT MIND — extraction contract as a living ledger.
//!
//! Mirrors `v6/sprefa-store/src/tasks.rs`: traits are the contract, the `Tasks`
//! impl bodies are `todo!()`, the DOCS are the task notes, `ExtractPlan` is the
//! proof-token epic ledger. Read the plan off this file.
//!
//! ════════════════════════════════════════════════════════════════════════
//! CURRENT MIND — session 2026-07-23. What this crate IS, after the rulings.
//! ════════════════════════════════════════════════════════════════════════
//!
//! Job (one leaf below the store): repo rev -> graph facts. Pure CPU + rayon,
//! arena-mastered. NO database, NO reactivity, NO async facade. Reactivity, the
//! async-eval flip, and the sprefa-language are other crates / another session.
//!
//! Scope (this layer owns all four):
//!   git-fu       rev -> file blobs. Shellout (`git cat-file --batch` bulk read,
//!                `cat-file -e` existence probe, `rev-parse`) — the v5 lab found
//!                shellout usually beats libgit2 on big histories (linux-kernel-
//!                class revs back in time). v5: `engine/repo.rs:1169` `read(rev,
//!                path)`, `:1152` cat-file --batch, `:108` rev_parse, `:123`
//!                cat-file -e; `engine/revid.rs` Rev identity + the worktree `+`
//!                suffix + `GitOid` (a stored rev git can resolve).
//!   extraction   syntax/semantics families, per-language, rayon, arena-per-file.
//!   scip         Tier-1 resolution source, BIDIRECTIONAL wire + ratchet (D-scip-wire).
//!   tree-iter    tree-sitter integration points (the floor + the CST family).
//!
//! Identity (content-addressed):
//!   project = repo.   file = git path + content hash (BlobHash).
//!   phase-1 key  (BlobHash, lang, Mask)         — same bytes anywhere -> one extract.
//!   phase-2 key  (BlobHash, RepoDigest, Mask)   — repo state changes -> re-resolve.
//!
//! Decisions landed this session (type math is the spec; code stubs catch up):
//!   D-families   families are TYPE-LEVEL: `Family` trait + marker structs, not a
//!                `NodeKind`/`EdgeKind` sum. `Node<F>`/`Edge<F>`; the sums DELETE.
//!                (Orthogonal axes are not variants of one type; the store splits by
//!                family anyway, so flatten-then-resplit is wasted motion. v5 + the
//!                bundles already had per-family types; the sum was the false unification.)
//!   D-planes     2 planes: RESOLUTION (Type|Call|Module — SCIP-wire, ratchet-able,
//!                multi-source) + VALUE-FLOW (Df|Flow — native, AST-only, the part no
//!                SCIP tool produces and where this crate earns its keep).
//!   D-module     Module collapses: resolution half -> SCIP namespace edges (a file IS
//!                a namespace; SCIP's symbol scheme already nests modules); binding
//!                half -> aux side metadata. Not a standalone resolution family.
//!   D-scip-wire  SCIP is a BIDIRECTIONAL wire: `ScipOccurrence <-> Node<F>` both ways.
//!                Our AST facts project OUT to ScipOccurrence (joinable, ratchet-
//!                eligible); foreign indexers project IN. Round-trippable for the 3
//!                resolution families ONLY — df/flow/binding have no SCIP shape.
//!   D-ratchet    `merge` generalizes from "SCIP overrides" to per-fact best-producer-
//!                wins over N producers (`Ast`, `Scip(&indexer)`, `Ghcacher`). The
//!                `Producer` tag rides the bundle, not the row.
//!   D-sync-only  NO async facade. `_6_facade` (`ReactiveExtract`/`ProjectView`) is CUT.
//!                Pure CPU + rayon; nothing awaits. The engine wraps our sync
//!                `dispatch` in ITS spawn_blocking if it is async. SCIP build =
//!                `std::process::Command`. (tokio is available/safe but unreached here.)
//!   D-port-clean port + clean v5's roster (syn/oxc/tree-sitter) as-is; no buy-vs-buy gate.
//!   D-concrete   concrete structs until a second impl (crate-map practicality ruling).
//!
//! CPU trait factoring — one seam per orthogonal dimension, no fat trait:
//!   tool    `Parser`        syn / oxc / tree-sitter — one impl per backing engine
//!   family  `Project<F>`    phase 1: `Parsed -> FamilyBundle<F>`
//!           `Resolve<F>`    phase 2: `FamilyBundle<F> + RepoCx -> Vec<ProjectEdge>`
//!   binding `Source`        one row per lang: parser + per-family projectors + scip
//!   orch    `Dispatch`      ONE generic impl; rayon + arena-per-worker live here
//!   gitfu   `BlobSource`    rev -> blob (`GitShellout` / others; lab picks)
//!   scip    `ScipSource`    build (subprocess) + load (protobuf parse)
//!   (enum, not trait, for the closed vocabularies: per-family kind enums, `Producer`,
//!    `FamilyTag`. trait for the open extension points.)
//!
//! Frontier (deferred, evidence-gated):
//!   CLI oracle   ship an ast-grep/biome-shaped CLI from this crate as a purity-proof
//!                oracle against biome / oxc (esp. when doing oxc-class work). Lineage:
//!                the v3 parser-rayon perf labs. Parked until the port lands.
//!   git-fu lab   re-establish the efficient rev->blob story (shellout vs libgit2 vs
//!                pack-index direct) on linux-kernel-history class input. v5's results
//!                were confusing; shellout usually won. RELEASES GitFuLabbed.
//!   k-CFA / node-level types / CFG-dominators — extract or engine? (Evidence-gated.)
//!
//! Companion epic plan: `v6/plans/2026-07-23-sprefa-extract-golden-plan.md`.

use crate::_3_extract::_0_shape::{ProjectEdge, RawEdge, RawNode};
use crate::_3_extract::_1_mask::{FamilyMask, FileBundle, ProjectBundle};
use crate::_3_extract::_2_traits::{ExtractBudget, ExtractJob, ProjectCx, Source};
use crate::_3_extract::_4_scip::{ScipError, ScipIndex};

// Proof tokens RELEASED by an unlanded task (mirror store tasks.rs convention):
//   TypedFamilies  epic 0 : per-family Family trait + Node<F>/Edge<F>; sums deleted
//   GitFuLabbed    epic G : rev->blob shellout-vs-libgit2 lab (linux-kernel-history class)
//   Ported         epic P : v5 four families + SCIP ported behind Project<F>/Resolve<F>
//   Arened         epic 2 : arena-per-file RSS flat under N-worker parse
//   Merged         epic 3 : ratchet — per-fact best-producer-wins over N producers
//   Dispatched     epic 4 : rayon dispatch, no lock contention / livelock
//   FlowUnified    epic 5 : flow_edge promoted to typed Flow<F> edges
//   Evidence       frontier: a measurement that closes a question
pub struct TypedFamilies;
pub struct GitFuLabbed;
pub struct Ported;
pub struct Arened;
pub struct Merged;
pub struct Dispatched;
pub struct FlowUnified;
pub struct Evidence;

// =============================================================================
// git-fu — rev -> file bytes (the layer this crate owns per the scope ruling)
// =============================================================================
/// Read one file's bytes at one rev. The expected impl shells out (`git cat-file`
/// bulk + existence probe + rev-parse); the engine MAY pre-stage bytes and bypass.
/// v5 source: `engine/repo.rs:1169 read()`, `:1152 cat-file --batch`, `revid.rs`
/// Rev/`GitOid`. The efficient-vs-not story is a lab (frontier: GitFuLabbed).
pub trait BlobSource: Sync {
    /// `repo_root` is the worktree; `rev` is a git-resolvable object name; `path`
    /// is repo-relative. Returns the blob bytes, or None if absent at that rev.
    fn blob_at(&self, repo_root: &std::path::Path, rev: &str, path: &str) -> Option<Vec<u8>>;
}

// =============================================================================
// Trait · Extract — the contract surface (each method doc = the note)
// =============================================================================
/// The extraction contract. SYNC throughout. The engine calls `dispatch` with the
/// changed blobs + the active cone mask; extract returns normalized nodes + edges +
/// aux for exactly the masked families.
///
/// REVISION (this session): signatures below still use the seed's pre-refactor types
/// (`FileBundle`/`ProjectBundle`/`ProjectCx`). They become, per the decisions above:
///   extract_file    -> per-family `Project<F>::project` (one per family, masked)
///   resolve_project -> per-family `Resolve<F>::resolve` (call/type/module; df none)
///   ProjectCx       -> `RepoCx`            (project = repo)
///   merge           -> `ratchet(&[(Producer, ExtractOutput)]) -> ExtractOutput`
///   dispatch        -> unchanged shape     (the ONE generic rayon orchestrator)
pub trait Extract {
    /// OPEN · rayon fan-out over `jobs`, one arena per worker · oracle: v5
    /// extractors (syn/oxc/tree-sitter) byte-identical on the same corpus ·
    /// parity: byte-identical node/edge set vs v5 · THE RAM GUN (RSS flat).
    fn dispatch(
        &self,
        jobs: Vec<ExtractJob>,
        cx: &ProjectCx,
        sources: &[Source],
        budget: &ExtractBudget,
    ) -> Vec<ExtractOutput>;

    /// OPEN · phase 1: one parse, masked projections · cache key (blob, lang,
    /// mask) · oracle v5 `TypeLang::extract_bundle` · identical bytes = one hit.
    /// REVISION -> `Project<F>::project`.
    fn extract_file(&self, job: &ExtractJob, sources: &[Source]) -> FileBundle;

    /// OPEN · phase 2: cross-file resolution · cache key (blob, repo_digest,
    /// mask) · oracle v5 `ModuleResolver::edges` + type/call resolvers.
    /// REVISION -> `Resolve<F>::resolve` (RepoCx).
    fn resolve_project(
        &self,
        blob: &ExtractJob,
        file: &FileBundle,
        cx: &ProjectCx,
        sources: &[Source],
    ) -> ProjectBundle;

    /// OPEN · Tier 1: shell out the foreign indexer over `root` · oracle v5
    /// `scip_setup` INDEXERS · subprocess (`std::process`), never bespoke FFI.
    fn scip_build(&self, root: &std::path::Path, indexer: &'static str) -> Result<(), ScipError>;

    /// OPEN · Tier 1: parse index.scip -> diet ScipIndex · oracle v5
    /// `scip_import::load` · reload-gated by mtime.
    fn scip_load(&self, index_path: &std::path::Path) -> Result<ScipIndex, ScipError>;

    /// OPEN · the ratchet: per-fact best-producer-wins over N producers
    /// (`Ast` / `Scip` / `Ghcacher`). SCIP ground-truth for call/type/module
    /// resolution is ONE rule, not the whole policy. · oracle: producer agreement.
    /// REVISION -> `ratchet(&[(Producer, ExtractOutput)])`. releases `Merged`.
    fn merge(&self, scip: &ScipIndex, ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle;
}

/// What one blob's extraction yields. REVISION -> per-family `FamilyBundle<F>` vecs
/// (df/call/type/module) + aux; the flat `nodes: Vec<RawNode>` (which carried the
/// now-deleted `NodeKind` sum) goes away.
#[derive(Clone, Debug, Default)]
pub struct ExtractOutput {
    pub nodes: Vec<RawNode>,
    pub edges: Vec<RawEdge>,
    pub project_edges: Vec<ProjectEdge>,
    pub aux: AuxFacts,
}

/// The family side tables (bindings, import forms, param_pos, args, fields, lits,
/// loops, docs, consts). Per-occurrence/per-node attributes, NOT a plane.
#[derive(Clone, Debug, Default)]
pub struct AuxFacts;

/// SCIP resolution layered over AST facts. REVISION -> the ratchet output: a chosen
/// `ExtractOutput` per fact + the producer that won. `scip_resolution` generalizes
/// to "winning producer's edges."
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
    fn dispatch(
        &self,
        _jobs: Vec<ExtractJob>,
        _cx: &ProjectCx,
        _sources: &[Source],
        _budget: &ExtractBudget,
    ) -> Vec<ExtractOutput> {
        todo!("epic 4: rayon par_iter over jobs; each worker owns ParseArena; budget-cap RSS")
    }
    fn extract_file(&self, _job: &ExtractJob, _sources: &[Source]) -> FileBundle {
        todo!("epic 1: tiered parse -> Project<F>::project per masked family")
    }
    fn resolve_project(
        &self,
        _blob: &ExtractJob,
        _file: &FileBundle,
        _cx: &ProjectCx,
        _sources: &[Source],
    ) -> ProjectBundle {
        todo!("epic 1: Resolve<F>::resolve for call/type/module; df skipped")
    }
    fn scip_build(&self, _root: &std::path::Path, _indexer: &'static str) -> Result<(), ScipError> {
        todo!("epic 3: shell out rust-analyzer/scip-typescript/...; write index.scip")
    }
    fn scip_load(&self, _index_path: &std::path::Path) -> Result<ScipIndex, ScipError> {
        todo!("epic 3: parse index.scip -> diet ScipIndex (symbol/range/role/relations only)")
    }
    fn merge(&self, _scip: &ScipIndex, _ast: &[(FileBundle, ProjectBundle)]) -> MergedBundle {
        todo!("epic 3: ratchet(producers) per-fact best-wins; releases Merged")
    }
}

// =============================================================================
// The remaining plan, as a trait — proof-token methods for the open epics.
// =============================================================================
/// A method's ARGS are body predicates (facts released earlier); its RETURN is
/// the head predicate. Linear narrative ordering, not hard build deps. Epic 0 types
/// families; epic G labs git-fu; epic P ports; epic 2 masters RAM; epic 3 ratchets;
/// epic 4 proves parallelism; epic 5 promotes flow; the frontier measures the rest.
pub trait ExtractPlan {
    /// 0  families are type-level: `Family` trait + `Node<F>`/`Edge<F>`; `NodeKind`/
    ///    `EdgeKind` sums deleted; family discriminant is a flat `FamilyTag` at the
    ///    store seam + ratchet key only.
    fn families_typed(&self) -> TypedFamilies;
    /// G  rev->blob git-fu lab: shellout vs libgit2 vs pack-index direct, on
    ///    linux-kernel-history class input. v5: shellout usually won.
    fn git_fu_labbed(&self, proof: &TypedFamilies) -> GitFuLabbed;
    /// P  port v5's four families + SCIP behind `Project<F>`/`Resolve<F>`, normalized
    ///    (sym->span, kind-String->typed enum, one Span). Parity: byte-identical vs v5.
    fn v5_ported(&self, proof: &GitFuLabbed) -> Ported;
    /// 2  arena-per-file parse keeps RSS flat under N-worker rayon dispatch.
    fn arena_ram_mastered(&self, proof: &Ported) -> Arened;
    /// 3  the ratchet: per-fact best-producer-wins over N producers (Ast/Scip/Ghcacher).
    fn ratchet_proven(&self, proof: &Arened) -> Merged;
    /// 4  rayon dispatch over a real corpus hits no lock contention / livelock.
    fn parallel_dispatch_proven(&self, proof: &Merged) -> Dispatched;
    /// 5  flow_edge (v5 stdlib 5th family, std/flow.dl:89) promoted to typed
    ///    `Flow<F>` edges — the interprocedural value-flow union in the type system.
    fn flow_edge_promoted(&self, proof: &Dispatched) -> FlowUnified;
    /// frontier: k-CFA / node-level types / CFG-dominators — extract or engine?
    ///    CLI oracle (ast-grep/biome-shaped, v3 perf-lab lineage) parks here too.
    ///    Returns Evidence, not a shipped change.
    fn frontier(&self) -> Evidence;
}
