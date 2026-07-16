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

use super::{derive_family, DepKey, Family, OutRow};
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
    /// derived, in declaration order.
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
    /// returns the rerun names in declaration order. Families with untouched
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

    /// The current memoized rows for a family (post-cold or post-react), or
    /// `None` if it was never derived.
    pub(crate) fn rows(&self, name: &str) -> Option<&[OutRow]> {
        self.memo.get(name).map(|m| m.rows.as_slice())
    }
}

/// The set of input relations a derive read, projected from its per-row deps.
fn rel_footprint(deps: &HashSet<DepKey>) -> HashSet<&'static str> {
    deps.iter().map(|d| d.rel).collect()
}
