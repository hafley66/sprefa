//! Built-in source-relation families behind one trait.
//!
//! Each family (`changed`, `changed_line`, `created`, the analysis-derived
//! `agent` / `dl_diag` / `type_shape` / `type_lgg` / catalog families, the SCIP
//! importer `scip_*`, the clone proposers `propose_extract` / `propose_clone`,
//! and the embedding `similar`) used to be four loose pieces in engine.rs — a
//! `*_RELS` const, a `*_rel_decls()` fn, a `*_rels_used()` gate, and a
//! `refresh_*_rel()` method — wired by a hand-written fan-out repeated in `tick`,
//! `tick_paths`, `declare_builtins`, `all_builtin_decls`, and the reserved-name
//! guard. This module collapses that shape: one `RelKind` impl per family, one
//! `rel_kinds()` registry the call sites loop over. The refresh BODIES live here
//! too (not thin wrappers), so the code actually leaves engine.rs.
//!
//! The families are split across submodules by bucket — `git` (changed /
//! changed_line / created), `analysis` (agent / dl_diag / type_shape /
//! type_lgg), `catalog`, `propose`, `scip`, `embed` — each impl `RelKind` from
//! this module and reaching the shared `col` / `git_anchors` / `rekey` helpers
//! via `super::`.
//!
//! Adding a family is now: write a unit struct, impl `RelKind`, add it to
//! `rel_kinds()`. The five call sites pick it up for free.
//!
//! Contract a family must match to live here: a no-arg, whole-set
//! `refresh(eng) -> Ok(changed?)` that self-diffs against what is stored
//! (returns `Ok(false)` on the steady-state no-op). A family that should NOT
//! re-run on every incremental tick overrides `dirty(changed)` to gate on the
//! changed-path set (`ScipKind` gates on `index.scip`). Bodies that need more of
//! the `Engine` surface reach it through the `pub(crate)` read helpers
//! (`repo_roots` / `node_file_set` / `read_content` / `knn_rows`); bounding that
//! surface behind a `RelCtx` borrow struct is the deferred encapsulation step in
//! `plans/2026-06-30-engine-breakdown-proposal.md`. Families that still don't fit
//! — a delta refresh (spine/node/module), extracted args (every/clock
//! intervals), or a `()` return that always runs (builtin/type/call/dataflow/
//! doc/daemon/effect) — await the further staged trait extensions there.

use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::ast::{Col, Program, RelDecl, Type};
use crate::engine::{rels_used, Engine};

mod analysis;
mod catalog;
mod embed;
mod env;
pub mod extract_family;
mod filelines;
mod git;
mod perf;
mod propose;
mod querylog;
mod scip;
mod write_ledger;

use analysis::{AgentKind, DlDiagKind, TypeLggKind, TypeShapeKind};
use catalog::{CatalogKind, VerbCatalogKind};
use embed::EmbedKind;
use env::EnvKind;
use filelines::FileLinesKind;
use git::{ChangedKind, ChangedLineKind, CreatedKind, GitRefKind, RevBehindKind};
use perf::PerfKind;
use propose::{ProposeCloneKind, ProposeExtractKind};
use querylog::QueryLogKind;
use scip::ScipKind;
use write_ledger::WriteLedgerKind;

pub(crate) use extract_family::CallPathRefreshOutcome;
pub use extract_family::{
    extract_families, extract_families_paths_pre_node, extract_families_post_node,
    extract_families_pre_node, CallFamily, ExtractFamily, ModuleFamily, PathRefreshContext,
    RefreshOutcome, TypeFamily,
};

/// Refresh the `query_log` projection right now, outside the normal tick
/// cadence. The daemon `query`/`query_sql` RPC handlers and the LSP `dl/query`
/// handler call this immediately after `Engine::log_query` so a request that
/// reads `query_log` (directly via `? query_log(...)` or via raw SQL against
/// `rel_query_log`) sees its own and every prior request's row in the SAME
/// response cycle — a plain read-only RPC never triggers a tick on its own,
/// so relying on the lazy tick-scoped `used`/`dirty` gate alone would leave
/// `rel_query_log` stale between file-watch ticks. Ignoring the `used(prog)`
/// gate here is deliberate: a request was just logged, so the caller always
/// wants it visible, whether or not the loaded program itself references the
/// relation. Cost is one full re-diff of `_query_log` vs `rel_query_log`
/// (same shape as `rel_count`/`stmt_ms`'s per-refresh scan), which grows with
/// total logged history — acceptable for the append-only, no-retention design
/// this relation deliberately keeps.
pub fn refresh_query_log(eng: &Engine) -> Result<bool> {
    QueryLogKind.refresh(eng)
}

/// A built-in, git-derived relation family: its name(s), column schema, the
/// lazy-use gate, and the whole-set refresh.
pub trait RelKind: Sync {
    /// The relation name(s) this family owns. Reserved against user `.dl`
    /// programs; the `changed_source_rels` keys on an incremental tick.
    fn rels(&self) -> &'static [&'static str];
    /// Column schema, one `RelDecl` per name in `rels()`.
    fn decls(&self) -> Vec<RelDecl>;
    /// Phrase for the reserved-name bail message ("`<name>` is {phrase}").
    fn reserved_msg(&self) -> &'static str;
    /// Whole-set recompute against the engine's git state; `Ok(true)` iff the
    /// stored set changed (drives the `changed` flag / rebuild scope).
    fn refresh(&self, eng: &Engine) -> Result<bool>;
    /// Whether the program references any owned name (default: lazy `rels_used`).
    fn used(&self, prog: &Program) -> bool {
        rels_used(prog, self.rels())
    }
    /// Runs BEFORE the extract families instead of after them: the family's
    /// output is an INPUT to extraction. `ScipKind` overrides — the type/call
    /// resolve closures read `scip_ref`, so the index must load first or a
    /// fresh db's first tick extracts without it (and only heals on tick 2
    /// via the digest fold).
    fn pre_extract(&self) -> bool {
        false
    }
    /// Should an *incremental* tick (`tick_paths`) call `refresh`? Default: yes,
    /// every tick — the self-diffing families re-read and early-out on a no-op.
    /// `ScipKind` overrides to gate on `index.scip` moving: in the changed set,
    /// or (because a gitignored index never enters the corpus walk, so the
    /// changed set cannot see it) by a stat digest of the index file itself.
    /// Editing source code never forces a full SCIP-index reload. Not consulted
    /// on a full `tick` (which always refreshes every used family). `changed` is
    /// the set of repo-relative paths the incremental tick saw move; `eng`
    /// gives access to stored digests for out-of-corpus inputs.
    fn dirty(&self, _eng: &Engine, _changed: &HashSet<String>) -> bool {
        true
    }
    /// A bookkeeping family projects state the tick itself writes (`stmt_ms`
    /// timings, `rel_count` after a rebuild, `query_log`). Its motion still
    /// refreshes rows and re-derives dependents (perf rails must stay live),
    /// but `is_settled` treats it as timer-like: a tick whose only motion is
    /// its own bookkeeping must not keep the daemon's poll loop scheduling
    /// full ticks forever (the sink-loop that wrote GBs per boot).
    fn bookkeeping(&self) -> bool {
        false
    }
}

/// Whether `rel` belongs to a bookkeeping family (see `RelKind::bookkeeping`).
pub fn is_bookkeeping_rel(rel: &str) -> bool {
    rel_kinds()
        .iter()
        .any(|k| k.bookkeeping() && k.rels().contains(&rel))
}

/// Every git-derived built-in family, in declaration order. `tick`,
/// `tick_paths`, `declare_builtins`, `all_builtin_decls`, and the reserved-name
/// guard iterate THIS instead of repeating the family list.
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[
        &ChangedKind,
        &ChangedLineKind,
        &CreatedKind,
        &GitRefKind,
        &RevBehindKind,
        &AgentKind,
        &DlDiagKind,
        &TypeShapeKind,
        &TypeLggKind,
        &CatalogKind,
        &VerbCatalogKind,
        &ScipKind,
        &ProposeExtractKind,
        &ProposeCloneKind,
        &EmbedKind,
        &PerfKind,
        &QueryLogKind,
        &EnvKind,
        &FileLinesKind,
        &WriteLedgerKind,
    ]
}

/// Flattened column decls across the registry, for `all_builtin_decls` /
/// `declare_builtins`.
pub fn rel_kind_decls() -> Vec<RelDecl> {
    rel_kinds().iter().flat_map(|k| k.decls()).collect()
}

/// One plain column for a built-in decl.
pub(super) fn col(n: &str, t: Type) -> Col {
    Col::plain(n.to_string(), t)
}

/// The two anchors every git-derived family needs to re-key git's paths to
/// repo-relative: `(toplevel, canonical root)`. git prints the PHYSICAL toplevel
/// (macOS `/private/var`) while `--root` may be the symlink, so a path is joined
/// onto `toplevel` then stripped of `root`. `None` when the root isn't a git
/// repo — every caller then yields an empty relation, not an error.
pub(super) fn git_anchors(eng: &Engine) -> Option<(PathBuf, PathBuf)> {
    let out = Command::new("git")
        .arg("-C")
        .arg(&eng.root)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let toplevel = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    let root = std::fs::canonicalize(&eng.root).unwrap_or_else(|_| eng.root.clone());
    Some((toplevel, root))
}

/// Re-key one git-printed path to repo-relative: join onto `toplevel`, strip
/// `root`, normalize separators. `None` drops a path outside the root.
pub(super) fn rekey(toplevel: &Path, root: &Path, p: &str) -> Option<String> {
    toplevel
        .join(p)
        .strip_prefix(root)
        .ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}
