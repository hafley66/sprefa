//! The parity surface as a living task ledger — keep our mind in here.
//!
//! Theory (the roles we re-express in SQLite under a RAM budget):
//! - salsa  = the DEPS array, React-style reactive invalidation. Which rel READS
//!            which; dirty-propagation + early cutoff.  → Reconcile (control plane)
//! - dd     = DERIVED relations forming a reactivity/dataflow graph from source
//!            facts; Z-set incremental maintenance.       → Cascade (fact plane)
//! - values = the computed tuples, NODES in a materialized view (the on-disk
//!            tables).                                     → Reach gives their structure
//! SQLite masters all three: state on disk, RSS = page cache (a knob), Rust heap
//! ~0. salsa/dd are THEORY + ORACLE, not shipped — their resident runtimes are
//! the v5 36 GB-swap death we're killing. Parity with their functions = DONE.
//!
//! Execution model (why this is a daemon, not a batch compiler):
//! Soufflé / classic datalog RUN TO COMPLETION — eager full fixpoint. We do the
//! opposite: ASYNC / LAZY-BY-DEFAULT FRONTIER. Compute only what a consumer
//! demands — a rel materializes when a user asks or SUBSCRIBES (active demand),
//! or when an EXTERNAL event triggers it (react to specific event sets/types —
//! rx-like logic, e.g. "poll ghcacher while I'm active, not all the time"). The
//! frontier is batched and RESUMABLE (run / pause / resume are events; the
//! Temporal plane is the substrate). The lazy frontier primitive already exists
//! as `reconcile::dirty` (one-hop + early cutoff). Goal: a general REACTIVITY
//! language — datalog now, prolog later.
//!
//! "Useless" traits: declared, NOT wired. Each impl body is `todo!()` and its doc
//! IS the task note (SQLite home, oracle, built?, parity test, status). When a
//! task is done, the real impl in `algo.rs` replaces the note.
//!
//! DONE bar (read off this file):
//! - Reach     ✅ oracle+parity green (efficiency open: `scc_labels` closure)
//! - Cascade   ✅ oracle+parity green (the genesis, most mature)
//! - Reconcile ❌ salsa oracle not ported — the ONE graph-correctness gap
//! - Temporal  substrate, not graph parity (see bottom)
//! - GraphStore 🧪 Epic 1 LANDED: Layout + stamp + attach_with (lib.rs relstore) + measure_storage
//!   (measure.rs). Split-vs-collapsed bytes are live (`just storage`); Epic 2 retarget is gated on
//!   the StorageDelta decision in `GraphStorePlan` below.
#![allow(dead_code, unused_variables, async_fn_in_trait)]

// ─────────────────────────────────────────────────────────────────────────────
// Unification — the math (retraction vs salsa, written out so we stop re-deriving)
// ─────────────────────────────────────────────────────────────────────────────
// ONE semi-naive fixpoint over the derivation graph G. A "derive" = one frontier
// hop (the Δ, the derivative); the "integrate" = the fixpoint (the materialized
// value); strata = the dimensions you shift through. This is LITERALLY why
// McSherry's engine is named DIFFERENTIAL dataflow — it keeps dD/dt incrementally.
// Tree (Fiber / RxJS subscription tree) is the degenerate acyclic case of the same
// fixpoint → graph algos reign; everything is a graph of a graph.
//
// Node k has value v_k, inputs D(k). A delta Δv_i flows along i→k; k's delta
// Δv_k = ⊕ of contributions; carry C_k fires iff it matters; if it fires,
// v_k ← v_k ⊗ Δv_k and Δv_k emits to readers. Repeat until no C_k fires.
//
// Instance A · retraction / dd   (the counting semiring, ℤ):
//   v_k ∈ ℤ.  ⊕=+, ⊗=+.  Δv_k = Σ_{i→k} m_ik·Δv_i  (linear; m = join arity).
//   carry C_k = "w_k crossed 0"  (alive ⟺ w_k>0).  PURE ALGEBRA, no code. Cycles
//   converge via the counting lattice (fixpoint of a monotone linear map).
//
// Instance B · salsa   (arbitrary value + Boolean dirty-bit):
//   v_k = (dirty_k ∈ Bool, val_k ∈ A).  dirty_k ← ∨_{i→k} dirty_i  (any input moved).
//   if dirty: val_k ← CODE_k(val_{D(k)})  — ARBITRARY code (the rust, docker run).
//   carry C_k = "val_k changed" (digest ≠).  Cutoff = recompute matched → wave stops.
//
// Same machine, different carried algebra. CONVERGE when CODE_k is a linear sum
// (val_k = Σ m·val_i): salsa's recompute IS dd's weight-add, cutoff = weight-stable.
// DIVERGE the moment CODE_k is non-linear/external (parse, hash, docker): dd's
// addition can't maintain it → recompute (salsa), cutoff spares redundant runs.
// Reactivity = the WHEN (salsa/control: dirty, watch, wake, demand). Retraction =
// the WHAT-after (dd/fact: weight → fixpoint). Both are the one loop; the ONLY
// difference is whether the carried value is a SUM (dd) or a COMPUTED OBJECT (salsa).
// ─────────────────────────────────────────────────────────────────────────────

// =============================================================================
// Trait A · Reach — read-only graph queries over cx_dep (prune = reached)
// =============================================================================
pub trait Reach {
    async fn reaches_from(&self, start: i64) -> Vec<i64>;
    async fn reached_by(&self, target: i64) -> Vec<i64>;
    async fn multi_source_walk(
        &self,
        starts: &[(i64, i64, i64)],
        halt: Option<&[i64]>,
        depth_cap: Option<i64>,
    ) -> Vec<(i64, i64, i64)>;
    async fn multi_source_halt_bfs(&self, starts: &[(i64, i64)], halt: &[i64]) -> Vec<(i64, i64)>;
    async fn scc_labels(&self) -> Vec<(i64, i64)>;
    async fn build_condensed(&self) -> (); // real return: reach::Condensed
    async fn count_pairs(&self) -> i128;
}

// =============================================================================
// Trait B · Cascade — mutating Z-set over cx_row (prune = weight ≠ 0)
// =============================================================================
pub trait Cascade {
    async fn assert(&self, seeds: &[(i64, i64)]) -> u64;
    async fn retract(&self, seeds: &[(i64, i64)]) -> u64; // acyclic only
    async fn retract_scc(&self, seeds: &[(i64, i64)]) -> u64; // cycle-safe
    async fn retract_dred(&self, seeds: &[(i64, i64)]) -> u64;
    async fn retract_dred_cte(&self, seeds: &[(i64, i64)]) -> u64;
    async fn alive_keys(&self) -> Vec<i64>; // the answer bytes diff'd against the oracle
}

// =============================================================================
// Trait C · Reconcile — salsa-in-SQL digest plane (prune = digest moved)
// =============================================================================
pub trait Reconcile {
    async fn seed(&self, id: i64, digest: i64, deps: &[(i64, i64)], rev: i64);
    async fn mark_changed(&self, ids: &[i64], rev: i64);
    async fn dirty(&self) -> Vec<i64>; // the stale FRONTIER (one-hop, early cutoff)
    async fn verify(&self, id: i64, new_digest: i64, rev: i64) -> bool; // moved? ⇒ cutoff
}

// =============================================================================
// Trait D · GraphStore: node+edge storage both planes sit on (NOT graph parity)
// =============================================================================
// Open MEASUREMENT, not a rewrite. Two assumptions to test:
//   EDGES (SETTLED): cx_dep (engine.rs:119) and rx_dep (engine.rs:980) are
//   already the same shape (2-col WITHOUT ROWID, both directions indexed). One
//   `dep(from,to)` table serves both planes; only the arrow convention flips (cx
//   parent->child vs rx reader->read). Edge representation is a solved problem.
//   NODES (OPEN): collapsible to ONE table carrying the union of value columns
//   (weight for counting; digest/changed_at/verified_at for digest). The semiring
//   difference is COLUMNS, not a conflict. Friction is GRANULARITY: cx_row is
//   per-tuple (key = tag*1e9+id, engine.rs:48); rx_memo is per-rel (id = rel id).
//   Collapse unlocks when reconcile goes per-tuple (full salsa memoizes per
//   (rel,args), same domain as cx_row).
// DIRECTION: do NOT rewrite the lab into one unified table. Build a small STAMP
// harness: a fn emitting a node table (rowid-clustered dense key + a vec of value
// cols) and a dep table (WITHOUT ROWID + both-direction index + TEMP working
// set), so each measurement gets its own variant and stays separable. Measure
// split-vs-collapsed and per-tuple-vs-per-rel; gain is unknown, which is why we
// measure before committing. Lab tunings (rowid cluster, WITHOUT ROWID, both-
// direction index, TEMP working set) all survive a merge.
pub trait GraphStore {
    async fn create(&self, node_value_cols: &[&str], per_tuple: bool) -> ();
    async fn upsert_node(&self, key: i64, values: &[i64]) -> ();
    async fn upsert_edges(&self, edges: &[(i64, i64)]) -> ();
    async fn children(&self, key: i64) -> Vec<i64>; // forward traversal (cascade hits)
    async fn parents(&self, key: i64) -> Vec<i64>;  // reverse traversal (rederive / dirty)
}

// ─── 2026-07-22 · the GraphStore plan, AS A TRAIT (modus ponens: a task's output type
// is the premise the next task consumes; read it like a datalog/prolog rule set) ───
// One intent home: edit this trait in place; do not splinter a plans/ file. Strike a
// method by deleting it once its real impl lands. EPIC 1 is struck — it landed in
// relstore + measure (pointers inline); this trait now holds only Epic 2/3 + frontier.

// Real inputs the chain consumes (the live types, not plan-local copies):
//   Layout       = crate::relstore::Layout            (Split | Collapsed)
//   StorageDelta = crate::measure::StorageDelta        (split_bytes, collapsed_bytes)
//   Corpus       = crate::measure::benchgraph::MultiGraph
pub struct TableNames { pub cascade_node: &'static str, pub cascade_dep: &'static str,
                        pub reconcile_node: &'static str, pub reconcile_dep: &'static str }

// Proof tokens still RELEASED by an unlanded task and REQUIRED by a later one. The
// remaining chain:  StorageDelta ⊢ Decision ⊢ Retarget ⊢ Parity  (+ Evidence probes).
pub struct Decision;                       // epic2_gate : go/no-go on the retarget
pub struct Retarget { pub cascade_sites: usize, pub reconcile_sites: usize } // Epic 2/3 answer
pub struct Parity { pub agreement_ok: bool, pub covering_ok: bool }          // Epic 2 answer
pub struct Evidence;                       // frontier : a measurement that would close a question

// ── EPIC 1 · LANDED 2026-07-22 · stamp harness + storage-only measurement ──
//   stamp          -> crate::relstore::stamp(db, layout)
//                     Split delegates to cascade+reconcile create_schema VERBATIM; Collapsed
//                     emits g_node + g_edge(src,dst) + ix_g_edge_dst + the TEMP working set.
//   attach_with    -> crate::relstore::RelStore::attach_with(db, layout); attach now
//                     delegates to attach_with(_, Split).
//   1.1 split gold -> lib.rs relstore tests: stamp(Split) sqlite_master == the live
//                     cascade+reconcile create_schema pair, object-for-object (+ 6 names).
//   1.2 collapse   -> lib.rs relstore tests: g_node/g_edge round-trip, PK dedup.
//   measure        -> crate::measure::measure_storage(corpus) -> StorageDelta
//                     (`just storage` prints it on a 40x40 multi-relation corpus);
//                     scaled variant measure_storage_scaled(layers,width) streams
//                     multi-GB with Rust heap ~0.
//   RESULT (scaled): collapsed/split = 1.040 at 5.66 GB (82M nodes / 164M edges,
//                     +234 MB), and ~1.046 stable from 300K through 2M nodes.
//                     Collapsed is +4% at EVERY scale that matters — the small-
//                     corpus "collapsed wins" was fixed table-overhead. Storage
//                     does NOT justify the Epic 2 retarget; collapse's case rests
//                     on the per-tuple-reconcile granularity unlock (frontier:
//                     per_tuple_unlock_evidence), not bytes.
//   g_edge columns are src/dst, not the plan's from/to — `from` is a SQL reserved word
//   and Epic 2 retargets every cascade statement onto these names.

/// The REMAINING plan, as a trait. Epic 1 fed it the `StorageDelta` fact; Epic 2 is
/// gated on whether that delta justifies the retarget. A method's ARGS are the body
/// predicates (facts released earlier); its RETURN is the head predicate. Modus ponens:
/// cannot fire `parity_under` without a `Retarget`, cannot get one without a `Decision`,
/// cannot decide without a `StorageDelta`.
///
/// Recon: RelStore (lib.rs) already unifies both planes over (rel,row) keys, so the layout
/// knob lives UNDER attach and the RelStore API + measure::run_cell stay fixed. cascade::
/// create_schema engine.rs:101; reconcile::create_schema engine.rs:972. Parity tests
/// through RelStore: tests/agreement.rs, tests/covering.rs.
pub trait GraphStorePlan {
    // ── EPIC 2 · retarget cascade+reconcile onto collapsed names (OPT-IN; gated) ──
    /// gate · decide whether the retarget is worth it given the measured `StorageDelta`.
    ///     The modus-ponens hinge: Epic 2 cannot fire without Epic 1's fact. Releases `Decision`.
    fn epic2_gate(&self, delta: crate::measure::StorageDelta) -> Decision;
    /// names_for(layout): Split cx_row/cx_dep/rx_memo/rx_dep; Collapsed g_node/g_edge x2.
    fn names_for(&self, layout: crate::relstore::Layout) -> TableNames;
    /// 2   retarget cascade.rs format! SQL (insert_rows/insert_deps/retract/retract_scc/
    ///     retract_dred/retract_dred_cte/assert/alive) onto `names`. Requires the gate
    ///     `Decision` (the "dont rewrite the whole lab" guard). Releases `Retarget`.
    async fn retarget_cascade_sql(&self, names: &TableNames, gate: Decision) -> Result<Retarget, sea_orm::DbErr>;
    /// 3   retarget reconcile.rs format! SQL (seed/mark_changed/dirty/verify) onto `names`.
    ///     Chains on `Retarget`. Releases the combined `Retarget`.
    async fn retarget_reconcile_sql(&self, names: &TableNames, prior: Retarget) -> Result<Retarget, sea_orm::DbErr>;
    /// 4   Golden + Done: agreement.rs + covering.rs green under the retargeted layout.
    ///     Releases `Parity`. Answers: does collapse hurt the op phase.
    async fn parity_under(&self, retargeted: &Retarget) -> Parity;

    // ── EPIC 3 · head-to-head sweep + the measurement record (no recommendation) ──
    /// 1   run each workload under Split + Collapsed. Requires `Retarget` (collapsed is live).
    async fn sweep(&self, cells: &[crate::measure::Cell], retargeted: &Retarget) -> Vec<crate::measure::RunRow>;
    /// 2   Golden + Done: out_hash equal across layouts per workload (correctness parity).
    async fn out_hashes_match(&self, rows: &[crate::measure::RunRow]) -> bool;

    // ── FRONTIER (deferred; each consumes a prior fact, returns the Evidence it still needs) ──
    /// per-tuple reconcile is the granularity unlock (rx_memo is per-rel today).
    fn per_tuple_unlock_evidence(&self, parity: &Parity) -> Evidence;
    /// INSTEAD-OF-trigger / view alias: keep SQL names, back one physical g_node (zero rename).
    fn trigger_alias_probe(&self, delta: crate::measure::StorageDelta) -> Evidence;
}
// ─── end 2026-07-22 GraphStore plan trait ───

// =============================================================================
// Substrate — NOT a graph-parity family. Documented here so we keep our mind.
// =============================================================================
// Temporal (engine.rs:1144-1299) — the bitemporal FACT plane:
//   fact(key, tt_from, tt_to, weight) WITHOUT ROWID; partial index ix_live
//   WHERE tt_to IS NULL. commit(deltas) = one JSON-batched txn: insert new live
//   facts, weight += dw, close (tt_to = rev) at weight <= 0 — the SAME Z-set
//   arithmetic as Cascade, plus full revision history. live()/digest() read the
//   tt_to IS NULL set.  commit:1224  live:1272  total_rows:1276  digest:1280
//   Role: the versioned substrate UNDER the graph — the home for the event-
//   sourced resumable frontier (run/pause/resume = revision-stamped facts;
//   tt_from/tt_to IS the event log). Not a salsa/dd function → no parity trait.
//   Consistency check = as-of replay (live() @ rev N == naive replay of 0..N),
//   storage self-consistency, not engine parity.

// =============================================================================
// The stub impl — every body is `todo!()`; the doc on each method IS the note.
// =============================================================================
pub struct Tasks;

impl Reach for Tasks {
    /// ✅ DONE-ish · SQLite `reach::reaches_from` :660 · oracle dd single-source ✅ · parity covering.rs
    async fn reaches_from(&self, _start: i64) -> Vec<i64> {
        todo!()
    }
    /// ✅ DONE-ish · SQLite `reach::reached_by` :676 · oracle dd reverse ⚠️ (trivial) · parity covering.rs
    async fn reached_by(&self, _target: i64) -> Vec<i64> {
        todo!()
    }
    /// ✅ · SQLite :692 · oracle v5 walk.rs (depth/tag) ✅ · parity covering.rs
    async fn multi_source_walk(
        &self,
        _starts: &[(i64, i64, i64)],
        _halt: Option<&[i64]>,
        _depth_cap: Option<i64>,
    ) -> Vec<(i64, i64, i64)> {
        todo!()
    }
    /// ✅ · SQLite :783 · oracle v5 walk.rs ✅ · parity covering.rs
    async fn multi_source_halt_bfs(&self, _starts: &[(i64, i64)], _halt: &[i64]) -> Vec<(i64, i64)> {
        todo!()
    }
    /// ✅ correct / ⚠️ efficiency · SQLite `reach::scc_labels` :797 · oracle dd fwd∩rev / v5 tarjan ✅ · parity covering.rs · THE QUADRATIC LEVER (materializes the full closure; re-lab pending)
    async fn scc_labels(&self) -> Vec<(i64, i64)> {
        todo!()
    }
    /// ✅ · SQLite `reach::build_condensed` :814 · oracle v5 derived ✅ · parity covering.rs
    async fn build_condensed(&self) -> () {
        todo!()
    }
    /// ✅ · SQLite `reach::count_pairs` :859 (condensation bitset) · oracle dd all-pairs ❌ NOT ported · parity covering.rs (v5 tarjan)
    async fn count_pairs(&self) -> i128 {
        todo!()
    }
}

impl Cascade for Tasks {
    /// ✅ · SQLite `cascade::assert` :352 · oracle dd (Z-set forward) ✅ · parity agreement.rs
    async fn assert(&self, _seeds: &[(i64, i64)]) -> u64 {
        todo!()
    }
    /// ✅ · SQLite `cascade::retract` :178 · oracle dd + oracle_survivors ✅ · parity agreement.rs · acyclic only
    async fn retract(&self, _seeds: &[(i64, i64)]) -> u64 {
        todo!()
    }
    /// ✅ · SQLite `cascade::retract_scc` :268 · oracle dd + oracle_survivors ✅ · parity agreement.rs · cycle-safe, beats DRed
    async fn retract_scc(&self, _seeds: &[(i64, i64)]) -> u64 {
        todo!()
    }
    /// ✅ · SQLite `cascade::retract_dred` :392 · oracle dd + oracle_survivors ✅ · parity agreement.rs
    async fn retract_dred(&self, _seeds: &[(i64, i64)]) -> u64 {
        todo!()
    }
    /// ✅ · SQLite `cascade::retract_dred_cte` :480 · oracle dd + oracle_survivors ✅ · parity agreement.rs (CTE = dead end for speed, measured)
    async fn retract_dred_cte(&self, _seeds: &[(i64, i64)]) -> u64 {
        todo!()
    }
    /// ✅ · SQLite lib.rs `RelStore::alive_keys` · oracle oracle_survivors ✅ · parity agreement.rs (the answer bytes)
    async fn alive_keys(&self) -> Vec<i64> {
        todo!()
    }
}

impl Reconcile for Tasks {
    /// ❌ GAP · SQLite `reconcile::seed` :992 · oracle salsa ❌ NOT ported · parity unit only
    async fn seed(&self, _id: i64, _digest: i64, _deps: &[(i64, i64)], _rev: i64) {
        todo!()
    }
    /// ❌ GAP · SQLite `reconcile::mark_changed` :1013 · oracle salsa ❌ · parity unit only
    async fn mark_changed(&self, _ids: &[i64], _rev: i64) {
        todo!()
    }
    /// ❌ GAP · SQLite `reconcile::dirty` :1024 · oracle salsa ❌ · parity unit only · (the FRONTIER — salsa's red-green, one-hop + cutoff)
    async fn dirty(&self) -> Vec<i64> {
        todo!()
    }
    /// ❌ GAP · SQLite `reconcile::verify` :1040 · oracle salsa ❌ · parity unit only
    async fn verify(&self, _id: i64, _new_digest: i64, _rev: i64) -> bool {
        todo!()
    }
}

impl GraphStore for Tasks {
    /// 🧪 OPEN · stamp node+dep tables with the required properties · one variant per measurement · gain unknown
    async fn create(&self, _node_value_cols: &[&str], _per_tuple: bool) -> () {
        todo!()
    }
    /// 🧪 OPEN · upsert one node carrying whichever value cols this plane uses
    async fn upsert_node(&self, _key: i64, _values: &[i64]) -> () {
        todo!()
    }
    /// ✅ edges SETTLED · shape already shared (cx_dep/rx_dep); one dep table
    async fn upsert_edges(&self, _edges: &[(i64, i64)]) -> () {
        todo!()
    }
    /// ✅ · forward traversal = cascade hits (engine.rs:218) · index ix_*_child
    async fn children(&self, _key: i64) -> Vec<i64> {
        todo!()
    }
    /// ✅ · reverse traversal = rederive (engine.rs:434) / dirty join (engine.rs:1027)
    async fn parents(&self, _key: i64) -> Vec<i64> {
        todo!()
    }
}

// (Temporal is substrate, not a parity trait — see the note above. No impl here.)
