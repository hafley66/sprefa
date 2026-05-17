//! Phase 3: content-addressed memo with disk cold tier.
//!
//! Replaces the unbounded in-RAM result cache. A memo entry is keyed
//! by `(owner_op_id, in_key)`. Validity is a generation comparison
//! against the recorded dependency set in `MEMO_DEPS` (Phase 2) — the
//! op is NEVER re-run to check freshness. On a HIT the stored output
//! rows are replayed downstream and the op's work is skipped entirely.
//!
//! Tiers (per the plan, no new unbounded structure):
//!   hot  — `StripedLru` (bounded, the existing cap machinery)
//!   cold — the `MEMO` fact table (SQLite when backed by it)

use std::sync::Arc;

use effect_runtime::v2::FactStore;

use crate::source_clock::{FactStoreClock, SourceClock, SourceId};
use crate::store::StripedLru;
use crate::Cursor;

/// Cold-tier table. One row per `(owner_op_id, in_key)`.
pub const MEMO_TABLE: &str = "_memo";

const DEFAULT_MEMO_HOT_CAP: usize = 8_192;

/// One memoized render. `out_rows` is the replayable output; `out_keys`
/// are the per-row key-hash hexes (Phase 0). `dep_fp` fingerprints the
/// `(SourceId, gen)` set the render read. `computed_gen` is the expand
/// tick the entry was produced at (telemetry / tie-break only).
#[derive(Clone, Debug)]
pub struct MemoVal {
    pub out_rows: Vec<Cursor>,
    pub out_keys: Vec<String>,
    pub dep_fp: [u8; 32],
    pub computed_gen: u64,
}

/// `None` ⇒ miss (never computed). `Some(v, false)` ⇒ replay
/// `v.out_rows`, do NOT run the op. `Some(v, true)` ⇒ stale, the op
/// must re-run; `v` is the prior value (for Phase 4 reconcile).
pub type Probe = Option<(MemoVal, bool)>;

pub struct Memo {
    facts: Arc<dyn FactStore<Cursor>>,
    clock: Arc<FactStoreClock>,
    hot: StripedLru<[u8; 32], MemoVal>,
}

fn hex32(b: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for x in b {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

/// `(owner_op_id, in_key)` → hot-tier key and PK column value.
fn entry_key(owner_op_id: &str, in_key: &str) -> ([u8; 32], String) {
    let mut h = blake3::Hasher::new();
    h.update(owner_op_id.as_bytes());
    h.update(b"\0");
    h.update(in_key.as_bytes());
    let digest = *h.finalize().as_bytes();
    (digest, hex32(&digest))
}

/// Length-prefixed concat of `cursor_codec::encode`, hex-encoded so it
/// rides a text fact column. Reverses on `decode_rows`.
fn encode_rows(rows: &[Cursor]) -> String {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    for r in rows {
        let b = crate::cursor_codec::encode(r);
        buf.extend_from_slice(&(b.len() as u32).to_le_bytes());
        buf.extend_from_slice(&b);
    }
    let mut s = String::with_capacity(buf.len() * 2);
    for x in &buf {
        s.push_str(&format!("{x:02x}"));
    }
    s
}

fn decode_rows(hex: &str) -> Vec<Cursor> {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect();
    if bytes.len() < 4 {
        return Vec::new();
    }
    let n = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(n);
    let mut p = 4usize;
    for _ in 0..n {
        if p + 4 > bytes.len() {
            break;
        }
        let len = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        if p + len > bytes.len() {
            break;
        }
        if let Ok(c) = crate::cursor_codec::decode(&bytes[p..p + len]) {
            out.push(c);
        }
        p += len;
    }
    out
}

impl Memo {
    pub fn new(facts: Arc<dyn FactStore<Cursor>>, clock: Arc<FactStoreClock>) -> Arc<Self> {
        Self::with_cap(facts, clock, DEFAULT_MEMO_HOT_CAP)
    }

    pub fn with_cap(
        facts: Arc<dyn FactStore<Cursor>>,
        clock: Arc<FactStoreClock>,
        hot_cap: usize,
    ) -> Arc<Self> {
        facts.declare(
            MEMO_TABLE,
            &["entry", "out_rows", "out_keys", "dep_fp", "computed_gen"],
        );
        Arc::new(Self {
            facts,
            clock,
            hot: StripedLru::new(hot_cap),
        })
    }

    fn cold_get(&self, pk: &str) -> Option<MemoVal> {
        let row = self
            .facts
            .read_where(MEMO_TABLE, "entry", pk)
            .into_iter()
            .next()?;
        let out_rows = decode_rows(row.get("out_rows").unwrap_or(""));
        let out_keys = row
            .get("out_keys")
            .unwrap_or("")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let dep_fp = {
            let h = row.get("dep_fp").unwrap_or("");
            let mut d = [0u8; 32];
            for (i, byte) in d.iter_mut().enumerate() {
                if let Some(s) = h.get(i * 2..i * 2 + 2) {
                    *byte = u8::from_str_radix(s, 16).unwrap_or(0);
                }
            }
            d
        };
        let computed_gen = row
            .get("computed_gen")
            .and_then(|g| g.parse::<u64>().ok())
            .unwrap_or(0);
        Some(MemoVal {
            out_rows,
            out_keys,
            dep_fp,
            computed_gen,
        })
    }

    /// Stale iff any recorded `MEMO_DEPS` gen differs from the source's
    /// current clock gen. Never re-runs the op to check.
    fn is_stale(&self, recorded_deps: &[(SourceId, u64)]) -> bool {
        recorded_deps
            .iter()
            .any(|(s, seen)| self.clock.current_gen(*s) != *seen)
    }

    /// Probe. `recorded_deps` is the `(SourceId, gen_seen)` set from
    /// `MEMO_DEPS` for this `(owner, in_key)` (Phase 2). Empty deps =>
    /// treat as a miss (never computed / not dep-tracked).
    pub fn probe(
        &self,
        owner_op_id: &str,
        in_key: &str,
        recorded_deps: &[(SourceId, u64)],
    ) -> Probe {
        let (hk, pk) = entry_key(owner_op_id, in_key);
        let val = self.hot.get(&hk).or_else(|| {
            let v = self.cold_get(&pk)?;
            self.hot.put(hk, v.clone());
            Some(v)
        })?;
        if recorded_deps.is_empty() {
            // No tracked deps: cannot prove freshness → force re-run,
            // but hand back the prior value for Phase 4 reconcile.
            return Some((val, true));
        }
        let stale = self.is_stale(recorded_deps);
        Some((val, stale))
    }

    pub fn put(
        &self,
        owner_op_id: &str,
        in_key: &str,
        recorded_deps: &[(SourceId, u64)],
        out_rows: Vec<Cursor>,
        out_keys: Vec<String>,
    ) {
        let mut h = blake3::Hasher::new();
        for (s, g) in recorded_deps {
            h.update(&s.0);
            h.update(&g.to_le_bytes());
        }
        let dep_fp = *h.finalize().as_bytes();
        let computed_gen = recorded_deps.iter().map(|(_, g)| *g).max().unwrap_or(0);
        let val = MemoVal {
            out_rows,
            out_keys,
            dep_fp,
            computed_gen,
        };
        let (hk, pk) = entry_key(owner_op_id, in_key);
        self.hot.put(hk, val.clone());

        self.facts
            .delete_matching(MEMO_TABLE, &[("entry", pk.as_str())]);
        let mut row = Cursor::default();
        row.set("entry", pk);
        row.set("out_rows", encode_rows(&val.out_rows));
        row.set("out_keys", val.out_keys.join(","));
        row.set("dep_fp", hex32(&val.dep_fp));
        row.set("computed_gen", val.computed_gen.to_string());
        self.facts.insert(MEMO_TABLE, Arc::new(row));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::MemFactStore;

    fn setup() -> (Arc<dyn FactStore<Cursor>>, Arc<FactStoreClock>, Arc<Memo>) {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let clock = FactStoreClock::new(facts.clone());
        let memo = Memo::new(facts.clone(), clock.clone());
        (facts, clock, memo)
    }

    fn cur(v: &str) -> Cursor {
        let mut c = Cursor::default();
        c.set_value(v);
        c
    }

    #[test]
    fn miss_then_hit_then_stale() {
        let (_f, clock, memo) = setup();
        let src = SourceId::for_file("/a.rs");
        let deps = vec![(src, clock.current_gen(src))];

        // Cold: never put → miss.
        assert!(memo.probe("owner", "k", &deps).is_none());

        memo.put(
            "owner",
            "k",
            &deps,
            vec![cur("row1"), cur("row2")],
            vec!["kh1".into(), "kh2".into()],
        );

        // Deps unchanged → HIT (replay, not stale).
        let (v, stale) = memo.probe("owner", "k", &deps).unwrap();
        assert!(!stale, "unchanged deps must be a replay HIT");
        assert_eq!(v.out_rows.len(), 2);
        assert_eq!(v.out_rows[0].value(), "row1");
        assert_eq!(v.out_keys, vec!["kh1", "kh2"]);

        // Bump the source → stale.
        clock.bump(src);
        let stale_deps = vec![(src, 0)]; // recorded gen still 0
        let (_v, stale) = memo.probe("owner", "k", &stale_deps).unwrap();
        assert!(stale, "bumped source must be stale");
    }

    #[test]
    fn cold_tier_survives_fresh_memo_over_same_store() {
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let clock = FactStoreClock::new(facts.clone());
        let src = SourceId::for_file("/a.rs");
        let deps = vec![(src, 0u64)];
        {
            let memo = Memo::new(facts.clone(), clock.clone());
            memo.put("o", "k", &deps, vec![cur("x")], vec!["kh".into()]);
        }
        // New Memo (cold hot tier) over the same store still replays.
        let memo2 = Memo::new(facts, clock);
        let (v, stale) = memo2.probe("o", "k", &deps).unwrap();
        assert!(!stale);
        assert_eq!(v.out_rows[0].value(), "x");
    }
}
