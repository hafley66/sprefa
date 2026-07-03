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
//! - Not every family reports a real change bool. `module`/`spine` do a
//!   wholesale rebuild with no self-diff, so their `refresh` returns
//!   `Ok(true)` whenever it runs (the tick marks their rels conservatively
//!   changed) — same contract as the hand-written blocks this replaces.
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
    /// Whole-corpus recompute. `Ok(true)` iff the tick should mark this
    /// family's rels changed — a real input-digest diff for `type`/`call`/
    /// `dataflow`/`doc`, unconditional `true` for the wholesale
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
    fn refresh(&self, eng: &mut Engine) -> Result<bool>;
    /// Whether the program references any owned name.
    fn used(&self, prog: &Program) -> bool {
        engine::rels_used(prog, self.rels())
    }
}

pub struct ModuleFamily;
pub struct TypeFamily;
pub struct CallFamily;
pub struct DataflowFamily;
pub struct DocFamily;
pub struct SpineFamily;

impl ExtractFamily for ModuleFamily {
    fn name(&self) -> &'static str { "module-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::MODULE_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::module_rel_decls() }
    fn reserved_msg(&self) -> &'static str { "a built-in module-graph relation" }
    fn digest_key(&self) -> Option<&'static str> { None }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> {
        eng.refresh_module_rels()?;
        // No change report from the wholesale rebuild, so the tick marks
        // these rels conservatively changed whenever the family runs.
        Ok(true)
    }
    fn used(&self, prog: &Program) -> bool { engine::module_rels_used(prog) }
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
    ) -> Result<bool> {
        if full_work {
            eng.refresh_module_rels_for_revs(&["WORK"])?;
        } else if !delta_paths.is_empty() {
            eng.refresh_module_rels_for_paths("WORK", delta_paths)?;
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}

impl ExtractFamily for TypeFamily {
    fn name(&self) -> &'static str { "type-rels" }
    fn rels(&self) -> &'static [&'static str] {
        // TYPE_RELS plus DOC_TEXT_RELS (doc_comment/doc_tag): one parse in
        // `refresh_type_rels` populates both sets, so a change marks both —
        // the pairing the hand-written tick blocks encoded as two `for`
        // loops under one refresh call.
        static RELS: std::sync::OnceLock<Vec<&'static str>> = std::sync::OnceLock::new();
        RELS.get_or_init(|| {
            engine::TYPE_RELS.iter().chain(engine::DOC_TEXT_RELS.iter()).copied().collect()
        })
    }
    fn decls(&self) -> Vec<RelDecl> {
        engine::type_rel_decls().into_iter().chain(engine::doc_text_rel_decls()).collect()
    }
    fn reserved_msg(&self) -> &'static str {
        "a built-in type-graph relation (type_edge / type_edge_rev / type_entity / type_sig / type_link)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:type") }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> { eng.refresh_type_rels() }
    fn used(&self, prog: &Program) -> bool {
        // The union the tick blocks used: doc_comment/doc_tag ride the same
        // parse (see rels() above), and the RelKind analysis families
        // type_shape/type_lgg consume type_entity, so they gate this refresh
        // too even though they own their rels elsewhere.
        engine::type_rels_used(prog)
            || engine::rels_used(prog, &["type_shape", "type_lgg"])
            || engine::doc_text_rels_used(prog)
    }
}

impl ExtractFamily for CallFamily {
    fn name(&self) -> &'static str { "call-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::CALL_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::call_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in call-graph relation (call_def / call_site / call_edge / call_edge_rev / call_name / call_kind)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:call") }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> { eng.refresh_call_rels() }
    fn used(&self, prog: &Program) -> bool { engine::call_rels_used(prog) }
}

impl ExtractFamily for DataflowFamily {
    fn name(&self) -> &'static str { "dataflow-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::DATAFLOW_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::dataflow_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in dataflow relation (df_node / df_edge / loop_over / allocates / nest / df_param / df_arg / df_field)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:dataflow") }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> { eng.refresh_dataflow_rels() }
    fn used(&self, prog: &Program) -> bool { engine::dataflow_rels_used(prog) }
}

impl ExtractFamily for DocFamily {
    fn name(&self) -> &'static str { "doc-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::DOC_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::doc_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in document relation (doc_node / doc_ref)"
    }
    fn digest_key(&self) -> Option<&'static str> { Some("extract:doc") }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> { eng.refresh_doc_rels() }
    fn used(&self, prog: &Program) -> bool { engine::doc_rels_used(prog) }
}

impl ExtractFamily for SpineFamily {
    fn name(&self) -> &'static str { "spine-rels" }
    fn rels(&self) -> &'static [&'static str] { &engine::SPINE_RELS }
    fn decls(&self) -> Vec<RelDecl> { engine::spine_rel_decls() }
    fn reserved_msg(&self) -> &'static str {
        "a built-in ref-spine relation (string / ref)"
    }
    fn digest_key(&self) -> Option<&'static str> { None }
    fn refresh(&self, eng: &mut Engine) -> Result<bool> {
        eng.refresh_spine_rels()?;
        // Wholesale projection with no change report; conservative mark.
        Ok(true)
    }
    fn used(&self, prog: &Program) -> bool { engine::spine_rels_used(prog) }
}

/// Every extraction-tied builtin rel family, in the order the full tick runs
/// them: module, type, call, dataflow, doc, then spine LAST (after the
/// hand-dispatched `node` refresh — see the module doc). The slice helpers
/// below split at the node seam so the tick paths never hand-index this.
pub fn extract_families() -> &'static [&'static dyn ExtractFamily] {
    &[&ModuleFamily, &TypeFamily, &CallFamily, &DataflowFamily, &DocFamily, &SpineFamily]
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

    /// The registry order IS the refresh order the hand-written blocks had:
    /// module first, spine last (after node); tick_paths' slice drops module.
    #[test]
    fn registry_order_matches_tick_order() {
        let names: Vec<&str> = extract_families().iter().map(|f| f.name()).collect();
        assert_eq!(names, ["module-rels", "type-rels", "call-rels",
                           "dataflow-rels", "doc-rels", "spine-rels"]);
        assert_eq!(extract_families_pre_node().len(), 5);
        assert_eq!(extract_families_paths_pre_node().len(), 4);
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
