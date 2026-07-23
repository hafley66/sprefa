//! The seams — what extraction IS. Four objects, in dependency order:
//!   Parser        — content -> arena CST (the parse tier; buys-vs-buys per lang)
//!   FileExtract   — (blob, lang, mask) -> FileBundle   [phase 1, content-addressed]
//!   ProjectExtract— (blob, fileset, mask) -> ProjectBundle [phase 2, project-scoped]
//!   ProjectCx     — the IO seam + lazy whole-project indexes (borrows, never owns)
//! plus `Source` (the per-language tiered binding of the three) and `ExtractBudget`
//! (the RAM + worker cap that keeps the crate from ever beachballing the machine).
//!
//! Two traits, kept two on purpose (delivery-plan ruling): the CACHE KEY differs.
//! FileExtract is a PURE function of content — identical bytes anywhere extract
//! once (key = blob + lang + mask). ProjectExtract depends on the file set — a
//! file appearing/disappearing can change a resolution (key = blob + project_digest
//! + mask). Forcing them into one trait would erase the demand-cone + dedup math
//! that is the whole reason the split exists. This is the one real distinction
//! the unification PRESERVES; everything else collapses onto the `_0_shape` rows.
//!
//! Prior art that pins this: Stack Graphs' index phase (per-file, isolated,
//! unresolved) vs query phase (merged, resolved) is this exact split; Kythe's
//! anchor (content-addressed) vs semantic node (name-resolved); SCIP's
//! Occurrence.range (content) vs Symbol string (name). The crate emits both, and
//! resolution crosses NO thread and NO file boundary inside phase 1.

use crate::_3_extract::_0_shape::{BlobHash, NameId};
use crate::_3_extract::_1_mask::{FamilyMask, FileBundle, ProjectBundle, ProjectDigest};
use std::path::Path;

// ── the IO seam ─────────────────────────────────────────────────────────────

/// Borrowed view over one (repo, rev) project, shared across a language's
/// phase-2 calls. Generalizes v5 `ProjectCx` (`modgraph/mod.rs:89`): the `reader`
/// closure is the IO injection point (extract never opens a file itself), and
/// the per-language `OnceLock` indexes are lazy whole-project state built once
/// per refresh. Extract is content-local; this is the ONLY handle it gets to the
/// world beyond the blob it was handed.
pub struct ProjectCx<'a> {
    /// Project-relative tracked file set (the resolution universe).
    pub files: &'a FileSet,
    /// Manifest path -> contents (Cargo.toml / package.json / go.mod). Feeds the
    /// per-language package indexes (RustCrates / ts_packages / GoIndex).
    pub manifests: &'a ManifestMap,
    /// Rev-correct content reader: given a project-relative path, return its
    /// bytes, or None. Injected by the engine; None in unit tests.
    pub reader: Option<&'a dyn Fn(&str) -> Option<Vec<u8>>>,
    /// The fold of `files` + `manifests` that invalidates phase-2 on change.
    pub digest: ProjectDigest,
    /// Lazy, per-language whole-project indexes (v5: RustCrates, KotlinIndex,
    /// GoIndex, ts_packages, python_roots). Built on first ask, reused across
    /// every phase-2 call in the refresh. Opaque here; each language module
    /// owns its concrete index type behind a `OnceLock`.
    pub indexes: IndexBag,
}

/// The file set: project-relative paths that exist at this rev. Resolution
/// succeeds only against this set (a specifier resolving outside it is
/// `Resolution::External` or `Unresolved`).
pub struct FileSet;
/// Manifest path -> raw manifest contents.
pub struct ManifestMap;
/// Type-erased bag of `OnceLock<LangIndex>` slots, one per language. The concrete
/// per-language index types live in the language modules (not in the type math).
pub struct IndexBag;

// ── phase 1 ─────────────────────────────────────────────────────────────────

/// "Here is a code file, get data" — a PURE function of content. Cache key
/// `(BlobHash, lang, FamilyMask)`. v5 `TypeLang` (`typegraph/mod.rs:439`) with
/// the mask promoted to the main method and the three per-family methods folded
/// into one bundle return. Every family has a phase-1 half; df lives HERE ONLY
/// (it has no phase-2 — no cross-file resolution).
pub trait FileExtract: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// One parse, many projections (v5 `extract_bundle` generalized to all langs,
    /// not just Rust). The mask selects which `Some(_)` arms the bundle carries.
    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> FileBundle;
}

// ── phase 2 ─────────────────────────────────────────────────────────────────

/// "Here is a codebase, get data" — depends on the file set. Cache key
/// `(BlobHash, ProjectDigest, FamilyMask)`. v5 `ModuleResolver`
/// (`modgraph/mod.rs:171`) generalized: every family that resolves across files
/// (call/type/module) has a phase-2 half that turns phase-1 specifiers/names into
/// resolved `ProjectEdge`s. df has none.
pub trait ProjectExtract: Send + Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// `file` is the phase-1 output for this blob; `cx` is the project view. The
    /// return is ONLY the cross-file resolutions for this one blob.
    fn resolve(
        &self,
        path: &str,
        file: &FileBundle,
        cx: &ProjectCx,
        mask: FamilyMask,
    ) -> ProjectBundle;
}

// ── the parse tier ──────────────────────────────────────────────────────────

/// content -> arena CST. One `Parser` impl per backing tool family (syn for
/// Rust, oxc for JS/TS, tree-sitter for the long tail). The arena is owned by
/// the caller (one per rayon task) and dropped after projection; the CST never
/// crosses a thread boundary (the oxc/biome arena-per-file pattern — the only
/// way to keep resident RAM flat across N parallel workers).
pub trait Parser: Sync {
    fn name(&self) -> &'static str;
    fn matches(&self, path: &str) -> bool;
    /// Parse into `arena`, returning a CST handle borrowed from it. The lifetime
    /// ties the tree to the arena; the caller projects to owned `_0_shape` rows
    /// and drops both. Body is the buy-vs-buy lab target (see plan epic 2).
    fn parse<'a>(&self, content: &[u8], arena: &'a ParseArena) -> Cst<'a>;
}

/// Bump arena for one file's parse. `bumpalo` or a typed arena; the budget
/// (`ExtractBudget.arena_bytes`) caps a single file's parse footprint.
pub struct ParseArena;
/// A lossy or lossless concrete syntax tree borrowed from a `ParseArena`. Opaque
/// in the type math; each `Parser` impl defines its own (oxc AST, syn tree,
/// tree-sitter CST). `lossless` flag: tree-sitter/rowan = yes (CST); oxc/syn = no.
pub struct Cst<'a> { _arena: std::marker::PhantomData<&'a ()> }

// ── the per-language source binding (the tier registry) ─────────────────────

/// How a language is served. v5 tiered this implicitly (Tier 1 SCIP for
/// Go/Python/C, Tier 2 native AST for Rust/Kotlin/TS, tree-sitter floor for
/// CST/comment extraction); v6 makes it a first-class, explicit binding. The
/// dispatcher merges tiers: SCIP is ground truth for call/type/module resolution;
/// AST fills dataflow (SCIP has no CFG/DDG) + everything when no SCIP index; the
/// tree-sitter floor covers the CST family and any lang with neither.
pub struct Source {
    pub parser: &'static dyn Parser,
    /// Native AST extraction (Tier 2). None => this lang relies on SCIP + floor.
    pub ast: Option<(&'static dyn FileExtract, &'static dyn ProjectExtract)>,
    /// SCIP indexer available for this lang (Tier 1). See `_4_scip`.
    pub scip: Option<ScipBinding>,
    // tree-sitter is the universal backup; always available via the floor
    // `Parser`, so it is not listed separately here.
}

/// Which SCIP indexer shells out for this language + how its output maps onto
/// the four families. `ScipSource` (in `_4_scip`) owns the subprocess/IPC; this
/// struct is the per-language static descriptor (indexer name, families covered).
pub struct ScipBinding {
    pub indexer: &'static str,             // e.g. "rust-analyzer scip", "scip-typescript"
    pub covers_resolution: ResolutionMask, // call/type/module (never df)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ResolutionMask { pub call: bool, pub type_: bool, pub module: bool }

// ── the budget (the v5 36GB-swap death, killed at the type level) ───────────

/// Hard caps that make extraction unable to beachball the machine, ever. The lab
/// stresses each to its breaking point (the point of the perf harness). Mirrors
/// sprefa-store's memcap (setrlimit/getrusage) + the rayon thread budget.
pub struct ExtractBudget {
    /// Process RSS ceiling; the run aborts a worker over it (memcap-style guard).
    pub rss_bytes: u64,
    /// Max arena bytes for ONE file's parse; a file over it is skipped + logged.
    pub arena_bytes: u64,
    /// rayon worker count. Capped (QoS/nice) so parse never starves the OS.
    pub workers: usize,
}

/// A blob the dispatcher is about to extract, with the mask the active cone asks
/// for. The dispatch input; rayon fans these out one-per-worker (epic 4).
pub struct ExtractJob {
    pub blob: BlobHash,
    pub path: NameId,   // for parser selection + diagnostics only; not an identity
    pub bytes: Vec<u8>,
    pub mask: FamilyMask,
}

impl ExtractJob {
    /// The cache key for phase 1: identical bytes + lang + mask hit one parse.
    pub fn file_key(&self, lang: &'static str) -> FileCacheKey {
        FileCacheKey { blob: self.blob, lang, mask: self.mask }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct FileCacheKey { pub blob: BlobHash, pub lang: &'static str, pub mask: FamilyMask }

/// A path this extractor recognizes, or not. Drives the `matches()` dispatch.
pub fn recognized(_sources: &[Source], _path: &Path) -> bool { false }
