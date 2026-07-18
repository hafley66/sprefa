//! `trait ExtractFamily`: the registry for the extraction-tied builtin rel
//! families that populate from parsed FILE CONTENT (not git state) — module
//! graph, type graph, call graph, dataflow, doc, ref-spine. Mirrors
//! `trait RelKind` (`src/rels/mod.rs`) in shape: one impl per family, one
//! registry the tick orchestrator loops over instead of a hand-written fan-out.
//! It stays a separate trait (engine breakdown Stage 4 / plan
//! `2026-07-02-engine-trait-refactor-v2.md`, Phase R1) because:
//!
//! - `refresh` takes `&mut Engine`. The type/call/dataflow refreshers mutate
//!   per-file fact caches (`type_facts_cache` / `call_facts_cache` /
//!   `df_facts_cache`), unlike `RelKind`'s `&Engine` git reads.
//! - Not every family reports an exact change. `module`/`spine` do a
//!   wholesale rebuild with no self-diff, so their `refresh` returns
//!   `Coarse` whenever it runs (the tick marks their rels conservatively
//!   changed) — same behavior as the hand-written blocks this replaces.
//! - Only `type`/`call`/`dataflow`/`doc` have a persisted `extract:<family>`
//!   input digest (perf gap A, the warm-tick skip that lives INSIDE each
//!   refresher); `module`/`spine` always re-derive when called. Hence
//!   `digest_key()` is `Option`.
//!
//! R1 is DISPATCH ONLY: every refresh body stays where it lives today
//! (`Engine::refresh_module_rels` / `refresh_type_rels` / `refresh_call_rels`
//! / `refresh_dataflow_rels` / `refresh_doc_rels` / `refresh_spine_rels` in
//! `engine/extract.rs`, plus the module `_for_revs`/`_for_paths` variants).
//! Body relocation is Phase R2. Likewise `decls()`/`reserved_msg()` delegate
//! to the existing decl tables + reserved-name guard in `engine/mod.rs`
//! rather than replacing those call sites: the guard's per-set messages
//! split doc_comment/doc_tag ("built-in doc relation") from the type-graph
//! set that `TypeFamily` owns as one refresh unit, so taking the guard over
//! is not a mechanical move (R2 scope; the plan's constraint-5 fallback).
//!
//! `type`/`call`/`dataflow` are three separate impls, not one bundled
//! family, even though the plan's prose groups them as one bucket: each has
//! its own used-gate, its own `extract:<family>` digest, and its own
//! per-file fact cache; merging them would make a program that reads only
//! `call_edge` also pay for a full type-entity + dataflow pass on its first
//! (cold-digest) tick.
//!
//! `node` (CST `node`/`child`) is deliberately NOT a member: it must run
//! BEFORE `spine` (its walk writes the `_strings`/`_where_bytes` meta tables
//! the spine projection reads) and its incremental form has a different
//! signature (`refresh_node_rels_delta` over the changed-path set), so it
//! stays hand-dispatched in both tick paths, sandwiched between this
//! registry's pre-node slice and its spine tail.

use anyhow::Result;
use std::collections::HashSet;

use crate::ast::{Program, RelDecl};
use crate::engine::{self, Engine};

/// What an extraction-family refresh can prove about its output. `Coarse`
/// preserves the legacy `true` contract: owned relations are attributed as
/// changed, but no exact row delta exists yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshOutcome {
    Unchanged,
    Coarse { reason: &'static str },
}

pub struct PathRefreshContext<'a> {
    pub changed_paths: &'a HashSet<String>,
    pub module_dependency_changed: bool,
    pub module_full_refresh: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallPathRefreshOutcome {
    Applied,
    Unchanged,
    Unsupported(&'static str),
}

impl RefreshOutcome {
    pub const LEGACY_REASON: &'static str = "legacy-family-refresh";

    pub fn from_legacy(moved: bool) -> Self {
        if moved {
            Self::Coarse { reason: Self::LEGACY_REASON }
        } else {
            Self::Unchanged
        }
    }

    pub fn moved(self) -> bool { !matches!(self, Self::Unchanged) }

    /// Exact outcomes require a generation-scoped staged delta, which the
    /// extraction contract does not have yet.
    pub fn is_exact(self) -> bool { false }
}

impl From<bool> for RefreshOutcome {
    fn from(moved: bool) -> Self { Self::from_legacy(moved) }
}

/// A builtin rel family populated by parsing the scanned file corpus (module
/// graph, type/call/dataflow graphs, doc, ref-spine) rather than by reading
/// git state (that shape is `RelKind`). See the module doc for why this
/// needs its own trait.
pub trait ExtractFamily: Sync {
    /// Short family name; doubles as the `--profile` phase label, keeping
    /// the exact strings the hand-written tick blocks printed.
    fn name(&self) -> &'static str;
    /// The relation name(s) this family owns — the set the tick marks
    /// changed when `refresh` returns true.
    fn rels(&self) -> &'static [&'static str];
    /// Column schema, delegating to the free-fn decl tables in
    /// `engine/mod.rs` (which `declare_builtins`/`all_builtin_decls` still
    /// call directly in R1 — see the module doc).
    fn decls(&self) -> Vec<RelDecl>;
    /// Phrase for a reserved-name bail message ("`<name>` is {phrase}").
    /// The hand-written guard in `declare_all` still owns reservation in R1
    /// (its doc_comment/doc_tag message differs from the type-graph one);
    /// this names the R2 takeover seam with the guard's exact phrases.
    fn reserved_msg(&self) -> &'static str;
    /// The persisted `extract:<family>` input-digest key (perf gap A), when
    /// this family has one. The skip itself stays inside the refresher;
    /// `None` for the wholesale `module`/`spine` rebuilds.
    fn digest_key(&self) -> Option<&'static str>;
    /// Whether this family's output is a pure function of file content alone, so
    /// the tick can skip it when no file moved (gated on `recon.changed`). Default
    /// false: `module` self-gates via its own input digest (`refresh_module_rels`),
    /// and `type`/`call`/`dataflow`/`doc` self-gate via their `extract:<f>` digests
    /// (and depend on the scip index, so they cannot be gated on file content
    /// alone). `spine` overrides true: it projects `_strings`/`_where_bytes`,
    /// which node walks only rewrite from file content.
    fn corpus_gated(&self) -> bool { false }
    /// Whole-corpus recompute. A moved outcome means the tick should mark this
    /// family's rels changed — a real input-digest diff for `type`/`call`/
    /// `dataflow`/`doc`, unconditional `Coarse` for the wholesale
    /// `module`/`spine` rebuilds (conservative mark). BOTH tick paths call
    /// this (the digest/self-diff inside each refresher is what makes an
    /// incremental no-op cheap); `module` alone has a different incremental
    /// entry point, `ModuleFamily::refresh_delta`, because its full-vs-delta
    /// decision needs the per-file classification (`module_full_work` /
    /// `module_delta_paths`) only `tick_paths`'s reconcile loop computes —
    /// so `tick_paths` iterates `extract_families_paths_pre_node` (module
    /// excluded) and dispatches module through `refresh_delta` at the same
    /// position the hand-written block held (after the clock refreshes,
    /// OUTSIDE the files-changed guard: a manifest-only change must still
    /// trigger it).
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome>;
    /// Watcher-path entry point. Object-safe so the existing family registry
    /// can dispatch it; families without a path specialization retain their
    /// current whole-family refresh behavior.
    fn refresh_paths(
        &self,
        eng: &mut Engine,
        _context: &PathRefreshContext<'_>,
    ) -> Result<RefreshOutcome> {
        self.refresh(eng)
    }
    /// Whether the program references any owned name.
    fn used(&self, prog: &Program) -> bool {
        engine::rels_used(prog, self.rels())
    }
    /// Cold-start staging (plan `2026-07-17-cold-start-staging.md`): may this
    /// family split into per-file shards, or must it run wholesale as one node?
    /// Default `false` (wholesale, `N_SHARDS=1`). No family sets this in the
    /// staging arc: the type/call/dataflow resolvers run a corpus-global name→def
    /// barrier and the `extract:<family>` skip digest is per-rev, so a per-file
    /// slice cannot be made digest-consistent without new infra (Shape B is a
    /// follow-up; see the cold_stage module doc).
    fn shardable_cold(&self) -> bool {
        false
    }
}

pub struct ModuleFamily;
pub struct TypeFamily;
pub struct CallFamily;
pub struct DataflowFamily;
pub struct DocFamily;
pub struct CommentFamily;
pub struct TemplateFamily;
pub struct UnresolvedFamily;
pub struct SpineFamily;

impl ExtractFamily for ModuleFamily {
    fn name(&self) -> &'static str { "module-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::MODULE_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::module_rel_decls() }
    fn reserved_msg(&self) -> &'static str { "a built-in module-graph relation" }
    fn digest_key(&self) -> Option<&'static str> { None }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_module_rels().map(RefreshOutcome::from_legacy)
    }
    // Win D: the type/call resolvers' import-scoped ambiguity narrowing reads
    // `module_edge_rev`, so this family must also run whenever type/call
    // (or their dependents) do; see `engine::module_rels_needed`'s doc.
    fn used(&self, prog: &Program) -> bool { engine::module_rels_needed(prog) }
}

impl ModuleFamily {
    /// The module family's incremental entry point (see `ExtractFamily::
    /// refresh`'s doc for why it is not a generic loop member): a manifest
    /// edit, a newly seen file, or a deletion (`full_work`) redoes the whole
    /// WORK rev (`refresh_module_rels_for_revs`); content-only edits on
    /// known files take the path-scoped delta
    /// (`refresh_module_rels_for_paths`). The classification booleans come
    /// precomputed from `tick_paths`'s per-file reconcile loop — that loop
    /// also feeds source-row retraction and the node delta, so it does not
    /// move here in R1; only the full-vs-delta DECISION does. `Ok(true)` iff
    /// any refresh ran (the caller then marks `rels()` changed, conservative
    /// like the full `refresh`).
    pub fn refresh_delta(
        &self,
        eng: &mut Engine,
        full_work: bool,
        delta_paths: &HashSet<String>,
    ) -> Result<RefreshOutcome> {
        if full_work {
            eng.refresh_module_rels_for_revs(&["WORK"])?;
        } else if !delta_paths.is_empty() {
            eng.refresh_module_rels_for_paths("WORK", delta_paths)?;
        } else {
            return Ok(RefreshOutcome::Unchanged);
        }
        Ok(RefreshOutcome::Coarse { reason: RefreshOutcome::LEGACY_REASON })
    }
}

impl ExtractFamily for TypeFamily {
    fn name(&self) -> &'static str { "type-rels" }
    fn rels(&self) -> &'static [&'static str] {
        // TYPE_RELS plus DOC_TEXT_RELS (doc_comment/doc_tag) plus
        // CONST_VALUE_RELS (const_value/const_value_rev): one parse in
        // `refresh_type_rels` populates all three sets, so a change marks
        // all of them — the pairing the hand-written tick blocks encoded as
        // several `for` loops under one refresh call.
        static RELS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
        RELS.get_or_init(|| {
            engine::TYPE_RELS.iter()
                .chain(engine::DOC_TEXT_RELS.iter())
                .chain(engine::CONST_VALUE_RELS.iter())
                .copied().collect()
        })
    }
    fn decls(&self) -> Vec<RelDecl> {
        engine::type_rel_decls().into_iter()
            .chain(engine::doc_text_rel_decls())
            .chain(engine::const_value_rel_decls())
            .collect()
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in type-graph relation (type_edge / type_edge_rev / type_entity / type_entity_rev / type_sig / type_link / type_link_rev)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:type") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_type_rels().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool {
        // The union the tick blocks used: doc_comment/doc_tag and
        // const_value/const_value_rev ride the same parse (see rels() above),
        // and the RelKind analysis families type_shape/type_lgg consume
        // type_entity, so they gate this refresh too even though they own
        // their rels elsewhere.
        engine::type_rels_used(prog)
            || engine::rels_used(prog, &["type_shape", "type_lgg"])
            || engine::doc_text_rels_used(prog)
            || engine::const_value_rels_used(prog)
    }
}

impl ExtractFamily for CallFamily {
    fn name(&self) -> &'static str { "call-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::CALL_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::call_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in call-graph relation (call_def / call_def_rev / call_site / call_edge / call_edge_rev / call_name / call_kind)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:call") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_call_rels().map(RefreshOutcome::from_legacy)
    }
    fn refresh_paths(
        &self,
        eng: &mut Engine,
        context: &PathRefreshContext<'_>,
    ) -> Result<RefreshOutcome> {
        match eng.refresh_call_rels_delta(context)? {
            CallPathRefreshOutcome::Applied => {
                Ok(RefreshOutcome::Coarse { reason: "call-owner-delta" })
            }
            CallPathRefreshOutcome::Unchanged => Ok(RefreshOutcome::Unchanged),
            CallPathRefreshOutcome::Unsupported(reason) => {
                tracing::debug!(reason = %reason, "[call-delta] fallback reason={reason} scope=call-family");
                self.refresh(eng)
            }
        }
    }
    fn used(&self, prog: &Program) -> bool { engine::call_rels_used(prog) }
}

impl ExtractFamily for DataflowFamily {
    fn name(&self) -> &'static str { "dataflow-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::DATAFLOW_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::dataflow_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in dataflow relation (df_node / df_node_rev / df_node_repo / df_node_repo_rev / df_edge / loop_over / allocates / nest / df_param / df_arg / df_arg_rev / df_field / df_field_rev / df_lit / df_lit_rev)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:dataflow") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_dataflow_rels().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool { engine::dataflow_rels_used(prog) }
    // Cold-start chunking (plan Addendum 2026-07-18): dataflow is the measured
    // hog (4.4s release: emit 2.3s + wholesale write 2.1s over 115k rows) and has
    // NO corpus-global resolver barrier — node ids are `file:line:col`-derived, so
    // a byte-bounded file slice emits exactly its own rows. Its cold node splits
    // into `cold_chunk_slices()` slices, each an `refresh_dataflow_rels_slice`
    // append; the family digest is saved once at the completion gate.
    fn shardable_cold(&self) -> bool { true }
}

impl ExtractFamily for DocFamily {
    fn name(&self) -> &'static str { "doc-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::DOC_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::doc_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in document relation (doc_node / doc_ref)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:doc") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_doc_rels().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool { engine::doc_rels_used(prog) }
}

impl ExtractFamily for CommentFamily {
    fn name(&self) -> &'static str { "comment-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::COMMENT_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::comment_rel_decls() }
    fn reserved_msg(&self) -> &'static str { "a built-in comment relation (comment_node)" }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:comment") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_comment_rels().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool { engine::comment_rels_used(prog) }
}

impl ExtractFamily for TemplateFamily {
    fn name(&self) -> &'static str { "template-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::TEMPLATE_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::template_rel_decls() }
    fn reserved_msg(&self) -> &'static str { "a built-in template-literal relation (template_parts)" }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:template") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_template_rels().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool { engine::template_rels_used(prog) }
}

impl ExtractFamily for UnresolvedFamily {
    fn name(&self) -> &'static str { "unresolved-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::UNRESOLVED_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::unresolved_rel_decls() }
    fn reserved_msg(&self) -> &'static str { "a built-in unresolved-marker relation (unresolved)" }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:unresolved") }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_unresolved_rel().map(RefreshOutcome::from_legacy)
    }
    fn used(&self, prog: &Program) -> bool { engine::unresolved_rels_used(prog) }
}

impl ExtractFamily for SpineFamily {
    fn name(&self) -> &'static str { "spine-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::SPINE_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::spine_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in ref-spine relation (string / ref)"
    }
    fn digest_key(&self) -> Option<&'static str> { None }
    fn corpus_gated(&self) -> bool { true }
    fn refresh(&self, eng: &mut Engine) -> Result<RefreshOutcome> {
        eng.refresh_spine_rels()?;
        // Wholesale projection with no change report; conservative mark.
        Ok(RefreshOutcome::Coarse { reason: RefreshOutcome::LEGACY_REASON })
    }
    fn used(&self, prog: &Program) -> bool { engine::spine_rels_used(prog) }
}

/// Every extraction-tied builtin rel family, in the order the full tick runs
/// them: module, type, call, dataflow, doc, comment, template, unresolved,
/// then spine LAST (after the hand-dispatched `node` refresh — see the module
/// doc). The slice helpers below split at the node seam so the tick paths
/// never hand-index this. `comment`/`template`/`unresolved` sit anywhere
/// between `module` (first) and `spine` (last): none writes spine tables nor
/// depends on another family, so both slices' module-first / spine-last
/// invariants hold.
pub fn extract_families() -> &'static [&'static dyn ExtractFamily] {
    &[&ModuleFamily, &TypeFamily, &CallFamily, &DataflowFamily, &DocFamily, &CommentFamily,
      &TemplateFamily, &UnresolvedFamily, &SpineFamily]
}

/// Full tick, before `node`: every family but the spine tail.
pub fn extract_families_pre_node() -> &'static [&'static dyn ExtractFamily] {
    let f = extract_families();
    &f[..f.len() - 1]
}

/// Incremental tick (`tick_paths`), before `node`: additionally drops the
/// leading `ModuleFamily`, which is dispatched there through
/// `ModuleFamily::refresh_delta` at its own (later) position.
pub fn extract_families_paths_pre_node() -> &'static [&'static dyn ExtractFamily] {
    let f = extract_families();
    &f[1..f.len() - 1]
}

/// Both ticks, after `node`: just `spine`, which projects the
/// `_strings`/`_where_bytes` rows the CST walk inserts.
pub fn extract_families_post_node() -> &'static [&'static dyn ExtractFamily] {
    let f = extract_families();
    &f[f.len() - 1..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bool_mapping_and_helpers_preserve_change_semantics() {
        assert_eq!(RefreshOutcome::from_legacy(false), RefreshOutcome::Unchanged);
        assert!(!RefreshOutcome::from(false).moved());
        assert!(!RefreshOutcome::Unchanged.is_exact());

        let moved = RefreshOutcome::from_legacy(true);
        assert_eq!(moved, RefreshOutcome::Coarse { reason: "legacy-family-refresh" });
        assert!(moved.moved());
        assert!(!moved.is_exact());
    }

    /// The registry order IS the refresh order the hand-written blocks had:
    /// module first, spine last (after node); tick_paths' slice drops module.
    #[test]
    fn registry_order_matches_tick_order() {
        let names: Vec<&str> = extract_families().iter().map(|f| f.name()).collect();
        assert_eq!(names, ["module-rels", "type-rels", "call-rels",
                           "dataflow-rels", "doc-rels", "comment-rels", "template-rels",
                           "unresolved-rels", "spine-rels"]);
        assert_eq!(extract_families_pre_node().len(), 8);
        assert_eq!(extract_families_paths_pre_node().len(), 7);
        assert_eq!(extract_families_paths_pre_node()[0].name(), "type-rels");
        assert_eq!(extract_families_post_node().len(), 1);
        assert_eq!(extract_families_post_node()[0].name(), "spine-rels");
    }

    /// Every family's decls cover exactly its rels (TypeFamily's set is
    /// TYPE_RELS ++ DOC_TEXT_RELS — one parse fills both).
    #[test]
    fn family_rels_match_decls() {
        for fam in extract_families() {
            let decl_names: Vec<String> =
                fam.decls().into_iter().map(|d| d.name).collect();
            let rel_names: Vec<String> =
                fam.rels().iter().map(|r| r.to_string()).collect();
            assert_eq!(decl_names, rel_names,
                "family {} decls/rels drift", fam.name());
        }
        assert!(TypeFamily.rels().contains(&"doc_comment"));
        assert!(TypeFamily.rels().contains(&"doc_tag"));
    }
}
