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

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use effect_runtime::v2::FactStore;

use crate::source_clock::{FactStoreClock, SourceClock, SourceId};
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
    /// Per-run in-RAM materialization of `_memo`, keyed by the `entry`
    /// PK. Loaded ONCE via `rows_of` (one query) on first access; every
    /// probe/replay/put is served from here. `None` = not yet loaded.
    /// This replaces the bounded `StripedLru` + per-probe `cold_get`
    /// `read_where`: a 63k-file run otherwise did ~55k sqlite point
    /// queries (LRU cap 8192 « corpus). `_memo` holds OUTPUT rows
    /// (≈6 MB on the linux fixture), bounded by result size not corpus.
    cells: Mutex<Option<HashMap<String, MemoVal>>>,
    /// PKs that existed in `_memo` at load time (need delete-before-
    /// insert on flush). A cold run starts empty ⇒ flush is pure
    /// `insert_batch`, zero deletes.
    loaded_pks: Mutex<HashSet<String>>,
    /// PKs written this run. Flush persists exactly these (cost ∝
    /// changes, not corpus): few on incremental, all on cold.
    dirty: Mutex<HashSet<String>>,
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

/// Decodes a legacy persisted `out_rows` hex blob. Replay is dead
/// (warm-skip + always-stale-on-probe), so nothing ENCODES anymore;
/// this stays only to read pre-change rows as empty/garbage-tolerant
/// without panicking on a fresh `--fact-db` carried over.
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
        _hot_cap: usize,
    ) -> Arc<Self> {
        facts.declare(
            MEMO_TABLE,
            &["entry", "out_rows", "out_keys", "dep_fp", "computed_gen"],
        );
        Arc::new(Self {
            facts,
            clock,
            cells: Mutex::new(None),
            loaded_pks: Mutex::new(HashSet::new()),
            dirty: Mutex::new(HashSet::new()),
        })
    }

    /// Decode one stored `_memo` row into `(pk, MemoVal)`.
    fn row_to_cell(row: &Cursor) -> Option<(String, MemoVal)> {
        let pk = row.get("entry")?.to_string();
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
        Some((
            pk,
            MemoVal {
                out_rows,
                out_keys,
                dep_fp,
                computed_gen,
            },
        ))
    }

    /// Bulk-load `_memo` ONCE into RAM (one `rows_of` query) and return
    /// the cell for `pk`. No per-probe `read_where`.
    fn cold_get(&self, pk: &str) -> Option<MemoVal> {
        let mut guard = self.cells.lock().unwrap();
        if guard.is_none() {
            let mut m: HashMap<String, MemoVal> = HashMap::new();
            let mut loaded = self.loaded_pks.lock().unwrap();
            for r in self.facts.rows_of(MEMO_TABLE) {
                if let Some((k, v)) = Self::row_to_cell(&r) {
                    loaded.insert(k.clone());
                    m.insert(k, v);
                }
            }
            *guard = Some(m);
        }
        guard.as_ref().unwrap().get(pk).cloned()
    }

    /// Stale iff any recorded `MEMO_DEPS` gen differs from the source's
    /// current clock gen. Never re-runs the op to check. LEGACY:
    /// retained for the gen-based probe; the CLI/seam path uses
    /// `is_stale_content` instead (clock-independent).
    fn is_stale(&self, recorded_deps: &[(SourceId, u64)]) -> bool {
        recorded_deps
            .iter()
            .any(|(s, seen)| self.clock.current_gen(*s) != *seen)
    }

    /// No-read content staleness. STALE iff ANY recorded dep's CURRENT
    /// `SourceIdentity` (git blob OID, or `stat` tuple, from the per-run
    /// `idx`) differs from the identity recorded when the component last
    /// read it. REPLAY iff every dep's identity is unchanged — and an
    /// unchanged identity NEVER opens the file (git OID is read from
    /// git's index; the `stat` fallback is one metadata call). The
    /// decision is independent of `FactStoreClock`/`gen_seen`/any event
    /// layer. Conservative: an empty path or empty recorded identity is
    /// STALE; a legacy untagged blake3 row decodes as `Bytes` and so is
    /// STALE vs any git/stat identity (one honest recompute re-records
    /// the tagged identity).
    fn is_stale_content(
        &self,
        recorded: &[(SourceId, u64, String, String)],
        idx: &crate::source_index::SourceIndex,
    ) -> bool {
        use crate::source_index::SourceIdentity;
        recorded.iter().any(|(_sid, _gen, recorded_enc, path)| {
            if recorded_enc.is_empty() || path.is_empty() {
                return true;
            }
            SourceIdentity::decode(recorded_enc)
                != idx.identity(std::path::Path::new(path))
        })
    }

    /// Probe. `recorded_deps` is the `(SourceId, gen_seen)` set from
    /// `MEMO_DEPS` for this `(owner, in_key)` (Phase 2). Empty deps =>
    /// treat as a miss (never computed / not dep-tracked). LEGACY
    /// gen-based path; kept for callers that pass no content hash.
    pub fn probe(
        &self,
        owner_op_id: &str,
        in_key: &str,
        recorded_deps: &[(SourceId, u64)],
    ) -> Probe {
        let (_hk, pk) = entry_key(owner_op_id, in_key);
        let val = self.cold_get(&pk)?;
        if val.out_rows.is_empty() {
            // No replayable payload persisted (blob dropped). Never
            // replay nothing — force recompute. Safe: warm-skip keeps
            // unchanged files out of probe entirely, so this only ever
            // fires for a changed file, which would recompute anyway.
            return Some((val, true));
        }
        if recorded_deps.is_empty() {
            // No tracked deps: cannot prove freshness → force re-run,
            // but hand back the prior value for Phase 4 reconcile.
            return Some((val, true));
        }
        let stale = self.is_stale(recorded_deps);
        Some((val, stale))
    }

    /// No-read content probe. `recorded` is the `(SourceId, gen_seen,
    /// content_id, source_path)` set from `MEMO_DEPS`, where `content_id`
    /// is the tagged `SourceIdentity` recorded last run. Staleness is
    /// decided by comparing it to the current identity from the per-run
    /// `idx` (git OID / stat) — no clock, no event bump, and NO file
    /// read for unchanged sources. This is the path the one-shot CLI
    /// seam uses so an edited file is detected from its identity moving
    /// while the 63k unchanged files are never opened.
    pub fn probe_ch(
        &self,
        owner_op_id: &str,
        in_key: &str,
        recorded: &[(SourceId, u64, String, String)],
        idx: &crate::source_index::SourceIndex,
    ) -> Probe {
        let (_hk, pk) = entry_key(owner_op_id, in_key);
        let val = self.cold_get(&pk)?;
        if val.out_rows.is_empty() {
            // No replayable payload (blob dropped) → force recompute,
            // never replay nothing. See `probe`.
            return Some((val, true));
        }
        if recorded.is_empty() {
            return Some((val, true));
        }
        let stale = self.is_stale_content(recorded, idx);
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
        let (_hk, pk) = entry_key(owner_op_id, in_key);
        // In-RAM only. NO per-call sqlite (that was the cold N+1:
        // 63k delete+insert+commit cycles). Persisted once by `flush`.
        let mut guard = self.cells.lock().unwrap();
        if guard.is_none() {
            let mut m: HashMap<String, MemoVal> = HashMap::new();
            let mut loaded = self.loaded_pks.lock().unwrap();
            for r in self.facts.rows_of(MEMO_TABLE) {
                if let Some((k, v)) = Self::row_to_cell(&r) {
                    loaded.insert(k.clone());
                    m.insert(k, v);
                }
            }
            *guard = Some(m);
        }
        guard.as_mut().unwrap().insert(pk.clone(), val);
        drop(guard);
        self.dirty.lock().unwrap().insert(pk);
    }

    /// Persist every entry written this run in ONE transaction. Cost ∝
    /// changes, not corpus: a cold run starts with an empty table so
    /// flush is a single `insert_batch` (zero deletes); an incremental
    /// run touches a handful of owners ⇒ a handful of `delete_matching`
    /// for the pks that previously existed, then one `insert_batch`.
    /// Idempotent; a no-op when nothing was written.
    pub fn flush(&self) {
        let dirty: Vec<String> = {
            let mut d = self.dirty.lock().unwrap();
            if d.is_empty() {
                return;
            }
            d.drain().collect()
        };
        let cells = self.cells.lock().unwrap();
        let map = match cells.as_ref() {
            Some(m) => m,
            None => return,
        };
        let mut loaded = self.loaded_pks.lock().unwrap();
        let mut rows: Vec<Arc<Cursor>> = Vec::with_capacity(dirty.len());
        for pk in &dirty {
            let val = match map.get(pk) {
                Some(v) => v,
                None => continue,
            };
            // ZERO-OUTPUT files (the ~59k of 63k that match nothing)
            // get NO `_memo` row at all. Absence of a row is the
            // canonical encoding of "computed, produced nothing": such
            // a file is warm-skipped next run via its `_memo_deps`
            // identity (never probed), so its `_memo` row was pure
            // dead storage + ~6 `sprf_strings` interns each. Persist a
            // row ONLY for output-bearing files — that preserves the
            // verified `Stale(empty)` reconcile path for the handful
            // of changed/matched files while removing the per-corpus
            // N+1 (~59k rows + ~1M interns) for the empty majority.
            if val.out_rows.is_empty() {
                // Transition output→empty (edit removed the last match):
                // drop the now-stale row so `_memo` stays consistent.
                // Hits retraction is independent (SUPPORT edges in
                // reconcile), so this only tidies bookkeeping.
                if loaded.contains(pk) {
                    self.facts
                        .delete_matching(MEMO_TABLE, &[("entry", pk.as_str())]);
                }
                continue;
            }
            if loaded.contains(pk) {
                self.facts
                    .delete_matching(MEMO_TABLE, &[("entry", pk.as_str())]);
            }
            let mut row = Cursor::default();
            row.set("entry", pk.clone());
            // Replay is dead (warm-skip + always-stale-on-probe); the
            // `out_rows`/`out_keys` blob carried zero information and was
            // the ~9s cold-flush / hex-doubled interned cell. Persist
            // empty — the row exists only for `dep_fp`/`computed_gen`.
            row.set("out_rows", "");
            row.set("out_keys", "");
            row.set("dep_fp", hex32(&val.dep_fp));
            row.set("computed_gen", val.computed_gen.to_string());
            rows.push(Arc::new(row));
            loaded.insert(pk.clone());
        }
        if !rows.is_empty() {
            self.facts.insert_batch(MEMO_TABLE, rows);
        }
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
    fn persisted_entry_survives_but_carries_no_replay_payload() {
        // CONTRACT CHANGE: replay is dead. Warm-skip keeps unchanged
        // files out of probe entirely; a probed (=changed) file is
        // always stale and recomputes. So the persisted `_memo` row
        // deliberately stores NO `out_rows`/`out_keys` blob (that was
        // the multi-KB hex cell interned per-file into `sprf_strings`,
        // ~1M execs / ~9s cold flush, for zero information). This test
        // pins the new truth: the entry SURVIVES a fresh Memo over the
        // same store (load works, no panic), but probing it forces
        // recompute (stale) and yields an empty replay payload —
        // NEVER a silent replay of nothing.
        let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
        let clock = FactStoreClock::new(facts.clone());
        let src = SourceId::for_file("/a.rs");
        let deps = vec![(src, 0u64)];
        {
            let memo = Memo::new(facts.clone(), clock.clone());
            memo.put("o", "k", &deps, vec![cur("x")], vec!["kh".into()]);
            memo.flush();
        }
        // Fresh Memo over the same store: the entry loads (Some, not a
        // miss → bookkeeping survived) but is forced stale with no
        // replay payload.
        let memo2 = Memo::new(facts, clock);
        let (v, stale) = memo2
            .probe("o", "k", &deps)
            .expect("persisted entry must still load (not a cold miss)");
        assert!(
            stale,
            "persisted entry must force recompute — replay payload is intentionally dropped"
        );
        assert!(
            v.out_rows.is_empty(),
            "no replay payload is persisted post-flush"
        );
    }

    // REWRITTEN for the no-read oracle. The previous version asserted
    // the blake3-of-bytes mechanic (recorded `h0 = blake3(content)`,
    // staleness re-reads + re-hashes). That mechanic was the 22s/13s
    // read-every-file defect; the oracle is now `SourceIdentity` (git
    // OID / stat, no read for unchanged files). Intent is PRESERVED and
    // still asserted: unchanged → REPLAY, edited → STALE, unreadable →
    // conservatively STALE, and the decision never consults the clock.
    // The tmp dir is non-git, so this exercises the `stat` path.
    #[test]
    fn no_read_staleness_detects_edits_and_replays_unchanged() {
        let dir = std::env::temp_dir().join(format!(
            "sprf_memo_ch_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.rs");
        std::fs::write(&f, b"fn a() {}\n").unwrap();
        let path = f.to_string_lossy().into_owned();

        let (_facts, _clock, memo) = setup();
        let idx = crate::source_index::SourceIndex::build(&dir);
        let src = SourceId::for_file(&path);
        // Record the identity the oracle currently sees (a stat tuple;
        // NO blake3 of bytes, NO file read).
        let id0 = idx.identity(&f).encode();
        let deps = vec![(src, 0u64, id0.clone(), path.clone())];

        // Cold miss → put under the recorded identity.
        assert!(memo.probe_ch("o", "k", &deps, &idx).is_none());
        memo.put(
            "o",
            "k",
            &[(src, 0u64)],
            vec![cur("r")],
            vec!["kh".into()],
        );

        // Unchanged file → REPLAY. Clock never bumped and the file is
        // NOT re-read (stat only) — proves no clock and no byte read.
        let (_v, stale) = memo.probe_ch("o", "k", &deps, &idx).unwrap();
        assert!(!stale, "unchanged file must REPLAY (no clock, no read)");

        // Edit on disk (size+mtime move) → STALE, detected with no read
        // and no clock bump / event layer.
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&f, b"fn a() {}\nfn b() {}\n").unwrap();
        let (_v, stale) = memo.probe_ch("o", "k", &deps, &idx).unwrap();
        assert!(
            stale,
            "edited file must be STALE from its identity moving"
        );

        // Unreadable path → conservatively STALE even with a pristine
        // clock (recorded stat tuple vs an empty/unresolvable identity).
        let bogus = vec![(src, 0u64, id0, "/no/such/path".to_string())];
        let (_v, stale) = memo.probe_ch("o", "k", &bogus, &idx).unwrap();
        assert!(stale, "unreadable source path is conservatively STALE");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
