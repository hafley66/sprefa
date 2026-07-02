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
mod git;
mod perf;
mod propose;
mod scip;

use analysis::{AgentKind, DlDiagKind, TypeLggKind, TypeShapeKind};
use catalog::CatalogKind;
use embed::EmbedKind;
use git::{ChangedKind, ChangedLineKind, CreatedKind, GitRefKind, RevBehindKind};
use perf::PerfKind;
use propose::{ProposeCloneKind, ProposeExtractKind};
use scip::ScipKind;

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
    /// Should an *incremental* tick (`tick_paths`) call `refresh`? Default: yes,
    /// every tick — the self-diffing families re-read and early-out on a no-op.
    /// `ScipKind` overrides to gate on `index.scip` being in the changed set, so
    /// editing source code never forces a full SCIP-index reload. Not consulted
    /// on a full `tick` (which always refreshes every used family). `changed` is
    /// the set of repo-relative paths the incremental tick saw move.
    fn dirty(&self, _changed: &HashSet<String>) -> bool {
        true
    }
}

/// Every git-derived built-in family, in declaration order. `tick`,
/// `tick_paths`, `declare_builtins`, `all_builtin_decls`, and the reserved-name
/// guard iterate THIS instead of repeating the family list.
pub fn rel_kinds() -> &'static [&'static dyn RelKind] {
    &[&ChangedKind, &ChangedLineKind, &CreatedKind, &GitRefKind, &RevBehindKind,
      &AgentKind, &DlDiagKind, &TypeShapeKind, &TypeLggKind, &CatalogKind,
      &ScipKind, &ProposeExtractKind, &ProposeCloneKind, &EmbedKind, &PerfKind]
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
    let out = Command::new("git").arg("-C").arg(&eng.root)
        .args(["rev-parse", "--show-toplevel"]).output().ok()?;
    if !out.status.success() { return None; }
    let toplevel = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    let root = std::fs::canonicalize(&eng.root).unwrap_or_else(|_| eng.root.clone());
    Some((toplevel, root))
}

/// Re-key one git-printed path to repo-relative: join onto `toplevel`, strip
/// `root`, normalize separators. `None` drops a path outside the root.
pub(super) fn rekey(toplevel: &Path, root: &Path, p: &str) -> Option<String> {
    toplevel.join(p).strip_prefix(root).ok()
        .map(|r| r.to_string_lossy().replace('\\', "/"))
}
