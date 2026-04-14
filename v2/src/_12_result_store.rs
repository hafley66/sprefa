//! Per-rule row store for cross-ref expansion (`${target.$VAR}` runtime).
//!
//! # Role
//!
//! This is the shared tape that lets one rule read another rule's output
//! at runtime. When rule `b` references `${a.$TAG}` somewhere in its body,
//! `expand_xrefs` (the Layer 2 adapter in `_5_op.rs`) needs the set of
//! cursor-capture maps that rule `a` has produced. `ResultStore` is where
//! rule `a` deposits them and rule `b` picks them up.
//!
//! # Shape
//!
//! One `RuleResult` per rule name. Each `RuleResult` holds:
//!   - `rows`: a `Vec<CaptureMap>` of every emitted cursor's captures,
//!     in emission order, duplicates preserved (no dedup at this layer).
//!   - `state`: atomic Pending → Complete flip that gates reads. The
//!     Layer 2 adapter sees `None` from `rows_of` until the producer rule
//!     has signalled it's finished emitting.
//!
//! # Write path (Layer 5 runner — not yet wired)
//!
//! The runner drives each rule's pipeline to completion. For every cursor
//! the rule emits, runner calls `append(rule, cursor.captures.clone())`.
//! When the rule's output stream drains, runner calls `mark_complete(rule)`.
//!
//! # Read path (Layer 2 adapter — wired)
//!
//! `expand_xrefs` reads via `rows_of(&xref.rule)` for each cross-ref the
//! consuming op declares. Level-based scheduling (also Layer 5) guarantees
//! that upstream rules in lower DAG levels finish before downstream rules
//! at higher levels start, so by the time the adapter reads, the producer
//! has already been marked complete. A `None` return in steady state is
//! therefore either a DAG misorder bug or the producer legitimately
//! emitted zero rows — the adapter treats both as empty-join.
//!
//! # What this store is NOT
//!
//! - It is not the canonical extraction table (that's `Writer`). Rows land
//!   here only to feed cross-rule joins at runtime; the writer still gets
//!   the authoritative persisted output.
//! - It does no dedup. `expand_xrefs` dedups at expansion time on the
//!   per-input `(var, value)` tuple basis. Source row multiplicity is
//!   preserved because two identical rows at the source level may gain
//!   distinguishing captures downstream.
//! - It has no per-run isolation yet. A fresh store is built per run by
//!   `OpCtx::fresh_xref_state`; lifecycle across LSP re-runs is a Layer 5
//!   decision (wipe vs diff vs retain).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use crate::_0_types::{Capture, Tri};

/// Snapshot of a single cursor's captures (`var → Capture`). One per row
/// in `RuleResult.rows`. Cloned out of `Cursor.captures` at emit time so
/// the store doesn't hold the rest of the cursor (path, evidence, content).
pub type CaptureMap = HashMap<Arc<str>, Capture>;

const PENDING:  u8 = 0;
const COMPLETE: u8 = 1;

/// One rule's accumulated output, plus a completion gate.
///
/// `rows` grows monotonically as the rule's pipeline emits cursors.
/// `state` flips Pending → Complete exactly once when the runner sees the
/// rule's stream end. The flip is the read barrier — `ResultStore::rows_of`
/// returns `None` while Pending, even if `rows` already has entries.
pub struct RuleResult {
    pub rows:  Mutex<Vec<CaptureMap>>,
    pub state: AtomicU8,
}

impl RuleResult {
    fn new() -> Self {
        Self { rows: Mutex::new(Vec::new()), state: AtomicU8::new(PENDING) }
    }
}

/// Shared per-run map of `rule_name → RuleResult`. Owned by the runner,
/// handed to every `OpCtx` via `Arc`. Lookups are O(1) under the outer
/// mutex; reads release the outer mutex before touching the inner row
/// vec, so a slow consumer doesn't block other rules' producers.
pub struct ResultStore {
    pub by_rule: Mutex<HashMap<Arc<str>, Arc<RuleResult>>>,
}

impl ResultStore {
    pub fn new() -> Self {
        Self { by_rule: Mutex::new(HashMap::new()) }
    }

    /// Runner-side. Append one cursor's captures to `rule`'s row list.
    /// Lazily creates the `RuleResult` entry if this is the first row.
    /// Thread-safe; producer side is single-rule-single-thread in the
    /// current design but the inner mutex tolerates concurrent appends.
    pub fn append(&self, rule: &Arc<str>, row: CaptureMap) {
        let entry = {
            let mut map = self.by_rule.lock().unwrap();
            map.entry(rule.clone())
                .or_insert_with(|| Arc::new(RuleResult::new()))
                .clone()
        };
        entry.rows.lock().unwrap().push(row);
    }

    /// Runner-side. Flip `rule`'s state to Complete. Idempotent. Until
    /// this is called, `rows_of(rule)` returns `None` regardless of how
    /// many rows have been appended. Consumer ops that hit `None` treat
    /// it as empty-join (drop cursor, emit `xref/empty-join` Hint).
    /// Lazily creates the `RuleResult` entry, so a rule that produces
    /// zero rows still gets marked Complete (consumers will see the
    /// empty Vec rather than the Pending sentinel).
    pub fn mark_complete(&self, rule: &Arc<str>) {
        let entry = {
            let mut map = self.by_rule.lock().unwrap();
            map.entry(rule.clone())
                .or_insert_with(|| Arc::new(RuleResult::new()))
                .clone()
        };
        entry.state.store(COMPLETE, Ordering::Release);
    }

    /// Returns a clone of all rows for `rule` once the producer has marked
    /// it complete. `None` if the rule is unknown or still pending.
    pub fn rows_of(&self, rule: &Arc<str>) -> Option<Vec<CaptureMap>> {
        let entry = {
            let map = self.by_rule.lock().unwrap();
            map.get(rule).cloned()
        }?;
        if entry.state.load(Ordering::Acquire) != COMPLETE { return None; }
        let rows = entry.rows.lock().unwrap().clone();
        Some(rows)
    }
}

impl Default for ResultStore {
    fn default() -> Self { Self::new() }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn cap(v: &str) -> Capture {
        Capture::new(Arc::from(v))
    }

    fn row(pairs: &[(&str, &str)]) -> CaptureMap {
        pairs.iter().map(|(k, v)| (Arc::<str>::from(*k), cap(v))).collect()
    }

    #[test]
    fn append_then_read_none_before_complete() {
        let s = ResultStore::new();
        let r: Arc<str> = Arc::from("foo");
        s.append(&r, row(&[("X", "1")]));
        s.append(&r, row(&[("X", "2")]));
        assert!(s.rows_of(&r).is_none());
    }

    #[test]
    fn append_then_read_after_complete() {
        let s = ResultStore::new();
        let r: Arc<str> = Arc::from("foo");
        s.append(&r, row(&[("X", "1")]));
        s.append(&r, row(&[("X", "2")]));
        s.mark_complete(&r);
        let rows = s.rows_of(&r).unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn unknown_rule_returns_none() {
        let s = ResultStore::new();
        let r: Arc<str> = Arc::from("nope");
        assert!(s.rows_of(&r).is_none());
    }

    #[test]
    fn multiple_rules_isolated() {
        let s = ResultStore::new();
        let a: Arc<str> = Arc::from("a");
        let b: Arc<str> = Arc::from("b");
        s.append(&a, row(&[("X", "1")]));
        s.append(&b, row(&[("Y", "9")]));
        s.mark_complete(&a);
        assert_eq!(s.rows_of(&a).unwrap().len(), 1);
        assert!(s.rows_of(&b).is_none());
        s.mark_complete(&b);
        assert_eq!(s.rows_of(&b).unwrap().len(), 1);
    }

    #[test]
    fn mark_complete_idempotent() {
        let s = ResultStore::new();
        let r: Arc<str> = Arc::from("foo");
        s.append(&r, row(&[("X", "1")]));
        s.mark_complete(&r);
        s.mark_complete(&r);
        assert_eq!(s.rows_of(&r).unwrap().len(), 1);
    }
}
