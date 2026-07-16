//! Reactive router — the memo layer over `Family` derives (plan
//! `2026-07-15-family-derive-reactive-engine.md`, the not-yet-built piece the
//! step-2 session left open).
//!
//! `derive_family` runs one family and reports the input deps it captured. The
//! router keeps one memo per family and, on a delta, reruns exactly the
//! families whose inputs were touched — the others keep their memoized rows.
//! This is the MobX/SolidJS `computed` reconciler: reads are tracked in `Ctx`,
//! the router decides who reruns.
//!
//! Skip granularity is the RELATION, not the row PK. Every family reads its
//! inputs via a full `Ctx::scan`, so an INSERT into a read relation must
//! retrigger it even though the new row's PK was absent from the prior derive's
//! per-row deps. The rel footprint (`{ dep.rel }`) is the sound skip key;
//! intersecting the raw PK-level deps would miss inserts. The per-PK `DepKey`
//! granularity stays useful for future point/filtered reads and for the
//! selectivity diagnostics the storage rails already assert.

// Wired into the live `refresh_call_rels` flip (Engine holds a persistent
// `FamilyRouter` across ticks). `cold` is exercised only by the storage rails
// (the live path calls `react`, whose None-branch subsumes the cold case), so
// allow dead_code for the non-test-only surface.
#![allow(dead_code)]

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use super::{derive_family, reconcile, DepKey, Family, OutRow, RowDelta};
use crate::db::Db;

/// Memoized state for one family: the rows it last emitted and the set of input
/// relations it read on that derive (its rel footprint).
struct FamilyMemo {
    rows: Vec<OutRow>,
    rels: HashSet<&'static str>,
}

/// Holds one memo per family; reruns only families whose rel footprint
/// intersects a delta's changed relations.
pub(crate) struct FamilyRouter<'f> {
    families: Vec<&'f dyn Family>,
    memo: HashMap<&'static str, FamilyMemo>,
}

impl<'f> FamilyRouter<'f> {
    pub(crate) fn new(families: Vec<&'f dyn Family>) -> Self {
        Self { families, memo: HashMap::new() }
    }

    /// Cold load: derive every family and populate the memo. Returns the names
    /// derived, in registry order (sorted by name).
    pub(crate) fn cold(&mut self, db: &Db) -> Result<Vec<&'static str>> {
        let mut derived = Vec::with_capacity(self.families.len());
        for family in &self.families {
            let (rows, deps) = derive_family(db, *family)?;
            self.memo.insert(family.name(), FamilyMemo { rows, rels: rel_footprint(&deps) });
            derived.push(family.name());
        }
        Ok(derived)
    }

    /// React to a delta described by the set of changed input relations. Reruns
    /// exactly the families whose last rel footprint intersects `changed`, plus
    /// any family with no memo yet (never cold-loaded). Updates their memo and
    /// returns the rerun names in registry order (sorted by name). Families with untouched
    /// inputs keep their memoized rows and are absent from the result — the
    /// skip is the point.
    pub(crate) fn react(
        &mut self,
        db: &Db,
        changed: &HashSet<&'static str>,
    ) -> Result<Vec<&'static str>> {
        let mut rerun = Vec::new();
        for family in &self.families {
            let name = family.name();
            let affected = match self.memo.get(name) {
                Some(memo) => !memo.rels.is_disjoint(changed),
                None => true, // never derived -> must derive
            };
            if !affected {
                continue;
            }
            let (rows, deps) = derive_family(db, *family)?;
            self.memo.insert(name, FamilyMemo { rows, rels: rel_footprint(&deps) });
            rerun.push(name);
        }
        Ok(rerun)
    }

    /// React AND reconcile: rerun the affected families and, for each, diff the
    /// fresh derivation against its memoized prior rows into a [`RowDelta`]
    /// (retract + insert), updating the memo. Returns EVERY rerun family, in
    /// registry order (sorted by name), including ones whose delta came back
    /// empty (a rerun that reproduced the same rows) — callers that only care
    /// about actual row movement filter on `RowDelta::is_empty`. Returning the
    /// full rerun set here mirrors `react`'s contract and preserves the
    /// rerun-name observability the skip-assertion tests rely on. This is the
    /// render input — the engine applies each delta incrementally instead of
    /// overwriting the relation, so a retracted input row surfaces as a
    /// retracted output row. A never-derived family reconciles against an
    /// empty base (delta = all inserts).
    pub(crate) fn react_deltas(
        &mut self,
        db: &Db,
        changed: &HashSet<&'static str>,
    ) -> Result<Vec<(&'static str, RowDelta)>> {
        let mut deltas = Vec::new();
        // Stage memo updates until every family derives successfully. A
        // mid-loop derive failure must not leave self.memo partially updated,
        // or later flips would reconcile against a memo that never matched the
        // table.
        let mut staged: Vec<(&'static str, FamilyMemo)> = Vec::new();
        for family in &self.families {
            let name = family.name();
            let affected = match self.memo.get(name) {
                Some(memo) => !memo.rels.is_disjoint(changed),
                None => true,
            };
            if !affected {
                continue;
            }
            let prev = self.memo.get(name).map(|memo| memo.rows.clone()).unwrap_or_default();
            let (rows, deps) = derive_family(db, *family)?;
            let delta = reconcile(&prev, rows.clone());
            staged.push((name, FamilyMemo { rows, rels: rel_footprint(&deps) }));
            deltas.push((name, delta));
        }
        for (name, memo) in staged {
            self.memo.insert(name, memo);
        }
        Ok(deltas)
    }

    /// The current memoized rows for a family (post-cold or post-react), or
    /// `None` if it was never derived.
    pub(crate) fn rows(&self, name: &str) -> Option<&[OutRow]> {
        self.memo.get(name).map(|m| m.rows.as_slice())
    }

    /// Drop the memo for the given family names. Used after a render failure so
    /// the next flip cold-reloads those families from a fresh derivation rather
    /// than reconciling against a memo that no longer matches the rolled-back
    /// table.
    pub(crate) fn forget(&mut self, names: &[&'static str]) {
        for name in names {
            self.memo.remove(name);
        }
    }

    /// The registered family with this name, so the render can read its
    /// `out_cols` and write the public rel generically (no per-family arm).
    pub(crate) fn family(&self, name: &str) -> Option<&'f dyn Family> {
        self.families.iter().copied().find(|family| family.name() == name)
    }
}

/// The set of input relations a derive read, projected from its per-row deps.
fn rel_footprint(deps: &HashSet<DepKey>) -> HashSet<&'static str> {
    deps.iter().map(|d| d.rel).collect()
}

// ---- T4 property-test accessors -------------------------------------------------
//
// `tests/it/retraction_props.rs` needs to inspect the router memo and to drive
// the flip with an explicit changed-set. The T4 plan restricts src changes to
// this file, so the hooks are surfaced on the public `Engine` type here.
// Integration-test crates link against the library compiled as a dependency,
// where `#[cfg(test)]` items are not emitted; therefore these are `pub` rather
// than `#[cfg(test)]`, but `#[doc(hidden)]` and `_test`-prefixed to discourage
// production use.
impl crate::engine::Engine {
    /// Current router memo rows for a call family, or `None` if the family has
    /// never been derived in this process. Test-only.
    #[doc(hidden)]
    pub fn call_router_memo_rows(&self, name: &str) -> Option<Vec<Vec<crate::ast::Value>>> {
        self.call_router
            .borrow()
            .as_ref()
            .and_then(|router| router.rows(name))
            .map(|rows| rows.iter().cloned().collect())
    }

    /// Family names that would rerun if `flip_call_rels_via_router` were called
    /// with `changed`. A cold router (no memo yet) reports every family.
    /// Test-only.
    #[doc(hidden)]
    pub fn call_router_rerun_names(
        &self,
        changed: &std::collections::HashSet<&'static str>,
    ) -> Vec<&'static str> {
        let router = self.call_router.borrow();
        let router = match router.as_ref() {
            Some(router) => router,
            None => {
                return crate::engine::family::call_families()
                    .iter()
                    .map(|family| family.name())
                    .collect();
            }
        };
        let mut rerun = Vec::new();
        for family in &router.families {
            let name = family.name();
            let affected = match router.memo.get(name) {
                Some(memo) => !memo.rels.is_disjoint(changed),
                None => true,
            };
            if affected {
                rerun.push(name);
            }
        }
        rerun
    }

    /// Test-only hook into `flip_call_rels_via_router` with an explicit
    /// changed-set. Returns the rerun family names. Test-only.
    #[doc(hidden)]
    pub fn flip_call_rels_via_router_test(
        &self,
        changed: &std::collections::HashSet<&'static str>,
    ) -> anyhow::Result<Vec<&'static str>> {
        self.flip_call_rels_via_router(changed)
    }
}
