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
//! `Retract` runs the Phase-5 COUNTED teardown
//! (`mounted_query::cascade_retract`): DRed decrements the row's
//! support `mult`; the sink row is deleted (and its support-children
//! descended) only when `sum(mult)` reaches 0. A row still derived by
//! another `(owner, in_key)` path survives.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use effect_runtime::v2::{MemoDelta, MemoProbe, MemoSeam, Next};

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
    /// Phase 6 (source-keyed owner identity). The INPUT cursor terms
    /// that derive this owner's read sources (e.g. `["FS"]` — the file
    /// path the `re`/`read`/`ast` op reads). When non-empty the memo
    /// `in_key` is `key_hash(in_source_terms)` of the input cursor:
    /// stable under edits to those sources (the focal `&`/`value`
    /// holding file bytes is NOT in the set, so an edit does not move
    /// the key). A content-threaded op then probes STALE (same owner
    /// key, newer source gen) instead of MISS, so `reconcile` fires.
    /// Empty ⇒ the pre-Phase-6 default `content_hash()` (== Phase-0
    /// `key_hash(&[])` for a no-dep op — the unchanged coarse default).
    in_source_terms: Vec<String>,
    /// Phase 6 (explicit capture-name keying — plan Decision 1/2). When
    /// non-empty, row IDENTITY is `key_hash(identity_terms)`: a fold of
    /// ONLY these terms (the regex capture names, e.g. `["NAME"]`).
    /// Everything else — the focal `&` carrying file bytes, `LO`/`HI`
    /// byte offsets, `MATCH`, `CONTENT_HASH` — is neither identity nor
    /// compared payload, so a span shift that keeps the capture stable
    /// is a NOOP, not a churned row. This is what makes the literal
    /// `fs>read>re` chain emit exactly ONE Retract + ONE Assert for the
    /// one renamed fn while every other match (whose byte offsets
    /// shifted) is untouched. Empty ⇒ the pre-Phase-6 model: identity =
    /// `val_hash(value_terms)` complement (whole-cursor when
    /// `value_terms` is also empty — the Ph0/Ph4 coarse default).
    identity_terms: Vec<String>,
    /// Telemetry for tests: (#Retract, #Assert) the seam decided.
    retracts: AtomicUsize,
    asserts: AtomicUsize,
    /// Times reconcile took the "same key, value moved" branch — i.e.
    /// a prior row was matched by IDENTITY and only its payload
    /// changed. Whole-cursor keying never hits this on a value edit
    /// (the whole-cursor identity itself moved); capture-name keying
    /// does. Distinguishes the two keyings for test 3.
    value_moved: AtomicUsize,
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
            in_source_terms: Vec::new(),
            identity_terms: Vec::new(),
            retracts: AtomicUsize::new(0),
            asserts: AtomicUsize::new(0),
            value_moved: AtomicUsize::new(0),
        })
    }

    /// Phase 6 constructor for a content-threaded `re`-style owner on
    /// the literal `fs > read > re` chain.
    ///   - `identity_terms`: the capture names (e.g. `["NAME"]`) —
    ///     row identity is exactly these, byte offsets/bytes ignored;
    ///   - `value_terms`: payload terms compared for the "value moved"
    ///     branch (empty ⇒ identity is the whole row signal, a span
    ///     shift with a stable capture is a NOOP);
    ///   - `in_source_terms`: input terms deriving the read source
    ///     (`["FS"]`) so the memo `in_key` is source-keyed (STALE on
    ///     edit, not MISS).
    pub fn with_capture_keying(
        graph: Arc<RuntimeGraph>,
        identity_terms: Vec<String>,
        value_terms: Vec<String>,
        in_source_terms: Vec<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            graph,
            value_terms,
            in_source_terms,
            identity_terms,
            retracts: AtomicUsize::new(0),
            asserts: AtomicUsize::new(0),
            value_moved: AtomicUsize::new(0),
        })
    }

    /// Phase 6: full constructor. `value_terms` = output payload terms
    /// (row identity is the complement). `in_source_terms` = INPUT
    /// cursor terms that derive this owner's read sources; non-empty
    /// makes the memo `in_key` source-keyed (stable across edits to
    /// those sources). For the `fs > read > re` chain the `re` owner
    /// uses `value_terms = [LO,HI,MATCH]`, `in_source_terms = [FS]`.
    pub fn with_terms(
        graph: Arc<RuntimeGraph>,
        value_terms: Vec<String>,
        in_source_terms: Vec<String>,
    ) -> Arc<Self> {
        Arc::new(Self {
            graph,
            value_terms,
            in_source_terms,
            identity_terms: Vec::new(),
            retracts: AtomicUsize::new(0),
            asserts: AtomicUsize::new(0),
            value_moved: AtomicUsize::new(0),
        })
    }

    pub fn retract_count(&self) -> usize {
        self.retracts.load(Ordering::SeqCst)
    }
    pub fn assert_count(&self) -> usize {
        self.asserts.load(Ordering::SeqCst)
    }
    pub fn value_moved_count(&self) -> usize {
        self.value_moved.load(Ordering::SeqCst)
    }

    fn value_refs(&self) -> Vec<&str> {
        self.value_terms.iter().map(|s| s.as_str()).collect()
    }

    /// Row identity. Phase 6: when `identity_terms` is set it is
    /// `key_hash(identity_terms)` — a fold of ONLY the capture names,
    /// so the focal `&`/bytes and `LO`/`HI` offsets are excluded and a
    /// span-shifted-but-same-capture row keeps its identity (and is a
    /// NOOP, not a churned row). Otherwise the pre-Phase-6 model: fold
    /// of every term NOT in `value_terms` (`val_hash` complement);
    /// empty `value_terms` ⇒ whole-cursor `key_hash(&[])` (== prior
    /// `content_hash`, the Ph0/Ph4 coarse default).
    fn identity_hex(&self, c: &Cursor, vt: &[&str]) -> String {
        if !self.identity_terms.is_empty() {
            let it: Vec<&str> = self.identity_terms.iter().map(|s| s.as_str()).collect();
            return hex32(&c.key_hash(&it));
        }
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

    /// Phase 6. The STABLE deriving-cursor projection for support
    /// bookkeeping. When `identity_terms` is set the deriving cursor
    /// for a `re`-style match is keyed by its capture names PLUS the
    /// source-deriving terms (`in_source_terms`, e.g. `FS`) — NOT its
    /// byte offsets or the focal bytes. Two renders of the same logical
    /// match (a span shift that keeps the capture stable, or an
    /// unchanged file replayed) then yield the SAME support cursor id,
    /// so `SupportLedger::add` is idempotent (mult stays 1) and
    /// `reconcile`'s Retract drives exactly the renamed match's mult to
    /// 0 while every span-shifted-but-stable match is untouched. With
    /// no `identity_terms` this is the identity cursor (whole cursor),
    /// preserving the Ph4/Ph5 whole-cursor support semantics.
    pub fn project_for_support(&self, c: &Cursor) -> Cursor {
        if self.identity_terms.is_empty() {
            return c.clone();
        }
        let mut keep: Vec<&str> = self.identity_terms.iter().map(|s| s.as_str()).collect();
        for t in &self.in_source_terms {
            keep.push(t.as_str());
        }
        let mut p = Cursor::default();
        for name in keep {
            if let Some(v) = c.get(name) {
                p.set(name, v);
            }
        }
        p
    }

    /// Phase 6. The support cursor id the ledger keys on for a deriving
    /// cursor — `blake3(encode(project_for_support(c)))` in hex. The
    /// downstream sink writer records support under THIS id; the
    /// `reconcile` teardown computes the SAME id from the prior cursor,
    /// so the retracted match's `mult` reaches 0 exactly.
    pub fn support_cursor_id(&self, c: &Cursor) -> String {
        let p = self.project_for_support(c);
        hex32(blake3::hash(&crate::cursor_codec::encode(&p)).as_bytes())
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
    fn in_key_for(&self, _owner: [u8; 32], raw: &Cursor) -> [u8; 32] {
        // Phase 6 (source-keyed owner identity). When the owner reads
        // sources whose paths live in `in_source_terms`, key the memo
        // on `key_hash(in_source_terms)` — a fold over ONLY those
        // identity terms. The focal `&`/`value` term (file bytes after
        // `read`) is excluded, so an edit to the file does NOT move the
        // key: the next render probes STALE (same owner key, newer
        // source gen) instead of MISS, and `reconcile` fires (this is
        // exactly what closes the Phase-4 `fs>read>re` retraction gap).
        // Empty ⇒ `content_hash()` — the pre-Phase-6 default, and for a
        // no-dep op identical to Phase-0 `key_hash(&[])`.
        if self.in_source_terms.is_empty() {
            return raw.content_hash();
        }
        let refs: Vec<&str> = self.in_source_terms.iter().map(|s| s.as_str()).collect();
        raw.key_hash(&refs)
    }

    fn probe(&self, owner: [u8; 32], in_key: [u8; 32]) -> MemoProbe<Cursor> {
        let owner_hex = hex32(&owner);
        let in_hex = hex32(&in_key);
        let deps = self.deps_of(&owner_hex, &in_hex);
        // Phase 6: an owner that recorded NO deps under the
        // source-keyed `in_key` (every owner in the literal chain
        // except the dep-recording `re` op — `fs`, `read`, the sink)
        // is a transparent PASS-THROUGH: always `Miss` so the op runs
        // and its rows flow, but `reconcile` neither counts nor tears
        // down (it just asserts the fresh rows). Only the MANAGED owner
        // (recorded deps) gets memo replay/stale + reconcile + DRed.
        // This scopes the retraction machinery to exactly the owner
        // whose source moved, without a per-op seam registry (that is
        // the Piece-3 stratify plumbing).
        if deps.is_empty() {
            return MemoProbe::Miss;
        }
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

        // Phase 6 pass-through: an owner that recorded no source deps
        // (the non-`re` stages of the literal chain) is not managed by
        // the retraction machinery. Splice every fresh row downstream
        // (Assert) WITHOUT counting telemetry, touching SUPPORT, or
        // writing a memo entry — exactly the pre-Phase-4 dispatch
        // behavior for that owner, just routed through the seam.
        if self.deps_of(&owner_hex, &in_hex).is_empty() {
            return (0..fresh.len()).map(MemoDelta::Assert).collect();
        }

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
                    self.value_moved.fetch_add(1, Ordering::SeqCst);
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

        // Counted (DRed) teardown: decrement the support `mult` of the
        // rows the new render no longer produces. A sink row is deleted
        // (and its support-children descended) only when its
        // `sum(mult)` reaches 0; a row still derived by another
        // `(owner, in_key)` path survives (Phase 5).
        if !retract_rows.is_empty() {
            // Phase 6: hand the teardown the STABLE support projection
            // of each retracted prior cursor (capture names + source
            // terms, no byte offsets / focal bytes), so
            // `cursor_storage_parts` inside `retract_memo_rows`
            // recomputes exactly the support cursor id the downstream
            // sink writer recorded — the renamed match's `mult` is
            // driven to 0 while span-shifted-but-stable matches keep
            // their support. With no `identity_terms` the projection is
            // the whole cursor (unchanged Ph4/Ph5 semantics).
            let projected: Vec<Arc<Cursor>> = retract_rows
                .iter()
                .map(|c| Arc::new(self.project_for_support(c.as_ref())))
                .collect();
            crate::mounted_query::retract_memo_rows(
                self.graph.facts.as_ref(),
                &owner_hex,
                &in_hex,
                &projected,
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
