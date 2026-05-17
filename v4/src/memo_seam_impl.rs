//! Phase 4: v4's implementation of the v3 `MemoSeam` + `reconcile`.
//!
//! The v3 driver (`expand`) probes this for EVERY component before
//! dispatch. It is backed by the Phase-3 `crate::memo::Memo` (hot
//! `StripedLru` + cold `MEMO` table) and the Phase-1 `SourceClock`,
//! both owned by the `RuntimeGraph`. Routing rule and non-rule ops
//! through one seam makes the Phase-3 rule replay one case of the
//! general mechanism (same memo store, same staleness rule).
//!
//! `reconcile` is the plan's Phase-E diff:
//!   same key & same value     → noop
//!   same key, value moved     → Retract(old) + Assert(new)
//!   key gone from fresh       → Retract
//!   key only in fresh         → Assert
//! `Retract` runs the existing PRESENCE-based teardown (delete the
//! sink rows whose support is gone) — identical semantics to the prior
//! `mounted_query::retract_supported_rows`. No mult/DRed; that is
//! Phase 5.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use effect_runtime::v2::{MemoDelta, MemoProbe, MemoSeam};

use crate::memo::MemoVal;
use crate::runtime_graph::RuntimeGraph;
use crate::source_clock::SourceId;
use crate::Cursor;

/// Hex of a 32-byte digest (the opaque key form the v3 seam hands us).
fn hex32(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// Parse a 64-char lowercase-hex `SourceId` (mirrors `rule.rs`).
fn sid_from_hex(hex: &str) -> Option<SourceId> {
    if hex.len() != 64 {
        return None;
    }
    let mut d = [0u8; 32];
    for (i, byte) in d.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(SourceId(d))
}

/// Pure row id: `blake3(owner ++ in_key ++ emit_ordinal)`. Stable
/// across runs for the same logical position (plan soundness:
/// `RowId` purity ⇒ memo replay is exact).
fn row_id(owner: &[u8; 32], in_key: &[u8; 32], ordinal: usize) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(owner);
    h.update(in_key);
    h.update(&(ordinal as u64).to_le_bytes());
    *h.finalize().as_bytes()
}

/// v4's `MemoSeam`. Carries the graph (clock + memo + facts) and the
/// op-declared `key_terms` for the owners this seam manages. A single
/// `key_terms` list is sufficient for one pipe's memoized op; a full
/// owner→key_terms registry is Phase-6 plumbing.
pub struct V4MemoSeam {
    graph: Arc<RuntimeGraph>,
    /// The op's VALUE/payload term names (`OperatorDef::key_terms()`).
    /// Row IDENTITY is the COMPLEMENT: `Cursor::val_hash(value_terms)`
    /// folds every term NOT in this list (the capture-name terms),
    /// `key_hash(value_terms)` is the payload. Empty ⇒ whole cursor is
    /// identity (coarse default).
    value_terms: Vec<String>,
    /// Telemetry for tests: (#Retract, #Assert) the seam decided.
    retracts: AtomicUsize,
    asserts: AtomicUsize,
}

impl V4MemoSeam {
    pub fn new(graph: Arc<RuntimeGraph>) -> Arc<Self> {
        Self::with_value_terms(graph, Vec::new())
    }

    /// `value_terms`: the memoized op's `OperatorDef::key_terms()` —
    /// its fixed VALUE/span terms (Decision 2: `re`→`[LO,HI,MATCH]`,
    /// `ast`/`json`→`[LO,HI]`). Identity is the complement. Empty =
    /// whole-cursor identity (coarse default, every other op).
    pub fn with_value_terms(graph: Arc<RuntimeGraph>, value_terms: Vec<String>) -> Arc<Self> {
        Arc::new(Self {
            graph,
            value_terms,
            retracts: AtomicUsize::new(0),
            asserts: AtomicUsize::new(0),
        })
    }

    pub fn retract_count(&self) -> usize {
        self.retracts.load(Ordering::SeqCst)
    }
    pub fn assert_count(&self) -> usize {
        self.asserts.load(Ordering::SeqCst)
    }

    fn value_refs(&self) -> Vec<&str> {
        self.value_terms.iter().map(|s| s.as_str()).collect()
    }

    /// Row identity = fold of every term NOT in `value_terms`
    /// (`val_hash` complement). Empty `value_terms` ⇒ whole-cursor
    /// `key_hash(&[])` (== prior `content_hash`).
    fn identity_hex(&self, c: &Cursor, vt: &[&str]) -> String {
        if vt.is_empty() {
            hex32(&c.key_hash(&[]))
        } else {
            hex32(&c.val_hash(vt))
        }
    }

    /// Row payload = the value subset (`value_terms`). Empty ⇒ the
    /// constant empty-fold (whole cursor is identity, no value subset).
    fn payload_hash(&self, c: &Cursor, vt: &[&str]) -> [u8; 32] {
        if vt.is_empty() {
            c.val_hash(&[])
        } else {
            c.key_hash(vt)
        }
    }

    fn deps_of(&self, owner_hex: &str, in_hex: &str) -> Vec<(SourceId, u64)> {
        self.graph
            .memo_deps_of(owner_hex, in_hex)
            .iter()
            .filter_map(|(h, g)| sid_from_hex(h).map(|s| (s, *g)))
            .collect()
    }
}

impl MemoSeam<Cursor> for V4MemoSeam {
    fn probe(&self, owner: [u8; 32], in_key: [u8; 32]) -> MemoProbe<Cursor> {
        let owner_hex = hex32(&owner);
        let in_hex = hex32(&in_key);
        let deps = self.deps_of(&owner_hex, &in_hex);
        match self.graph.memo().probe(&owner_hex, &in_hex, &deps) {
            None => MemoProbe::Miss,
            Some((val, false)) => {
                MemoProbe::Replay(val.out_rows.into_iter().map(Arc::new).collect())
            }
            Some((val, true)) => {
                MemoProbe::Stale(val.out_rows.into_iter().map(Arc::new).collect())
            }
        }
    }

    fn reconcile(
        &self,
        owner: [u8; 32],
        in_key: [u8; 32],
        prior: Option<Vec<Arc<Cursor>>>,
        fresh: &[Arc<Cursor>],
    ) -> Vec<MemoDelta> {
        let vt = self.value_refs();
        let owner_hex = hex32(&owner);
        let in_hex = hex32(&in_key);

        // prior: identity(hex) -> (row_id, payload_hash, prior cursor)
        let mut prior_map: BTreeMap<String, ([u8; 32], [u8; 32], Arc<Cursor>)> = BTreeMap::new();
        if let Some(prior_rows) = &prior {
            for (ord, c) in prior_rows.iter().enumerate() {
                let kh = self.identity_hex(c, &vt);
                let vh = self.payload_hash(c, &vt);
                let rid = row_id(&owner, &in_key, ord);
                prior_map.insert(kh, (rid, vh, c.clone()));
            }
        }

        let mut deltas: Vec<MemoDelta> = Vec::new();
        let mut retract_rows: Vec<Arc<Cursor>> = Vec::new();
        let out_keys: Vec<String> =
            fresh.iter().map(|c| self.identity_hex(c, &vt)).collect();

        for (ord, c) in fresh.iter().enumerate() {
            let kh = &out_keys[ord];
            let vh = self.payload_hash(c, &vt);
            let new_rid = row_id(&owner, &in_key, ord);
            match prior_map.remove(kh) {
                Some((old_rid, old_vh, _)) if old_rid == new_rid && old_vh == vh => {
                    // same key, same value → noop (stable row).
                }
                Some((old_rid, _, old_cur)) => {
                    // same key, value/position moved → Retract + Assert.
                    let _ = old_rid;
                    retract_rows.push(old_cur.clone());
                    deltas.push(MemoDelta::Retract(old_rid));
                    deltas.push(MemoDelta::Assert(ord));
                }
                None => {
                    // key only in fresh → Assert.
                    deltas.push(MemoDelta::Assert(ord));
                }
            }
        }
        // keys in prior but absent from fresh → Retract.
        for (_kh, (old_rid, _vh, old_cur)) in prior_map {
            retract_rows.push(old_cur);
            deltas.push(MemoDelta::Retract(old_rid));
        }

        // Presence-based teardown: delete the memoized rows the new
        // render no longer produces. Phase-4 keeps this presence-only
        // (a row is gone when its producing render stops emitting it),
        // exactly the prior `mounted_query` semantics. Phase 5 swaps
        // this for mult/DRed.
        if !retract_rows.is_empty() {
            crate::mounted_query::retract_memo_rows(
                self.graph.facts.as_ref(),
                &owner_hex,
                &in_hex,
                &retract_rows,
            );
        }

        // Record the new memo entry under the freshly-recorded deps so
        // the next unchanged run replays (op dispatch 0×).
        let fresh_deps = self.deps_of(&owner_hex, &in_hex);
        let val = MemoVal {
            out_rows: fresh.iter().map(|c| c.as_ref().clone()).collect(),
            out_keys: out_keys.clone(),
            dep_fp: [0u8; 32],
            computed_gen: 0,
        };
        self.graph.memo().put(
            &owner_hex,
            &in_hex,
            &fresh_deps,
            val.out_rows,
            val.out_keys,
        );

        let n_ret = deltas
            .iter()
            .filter(|d| matches!(d, MemoDelta::Retract(_)))
            .count();
        let n_asr = deltas
            .iter()
            .filter(|d| matches!(d, MemoDelta::Assert(_)))
            .count();
        self.retracts.fetch_add(n_ret, Ordering::SeqCst);
        self.asserts.fetch_add(n_asr, Ordering::SeqCst);
        deltas
    }
}
