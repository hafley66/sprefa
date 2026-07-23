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
//! - Reconcile ⚠ oracle PORTED (oracle::salsa + tests/reconcile.rs); parity test PROVED
//!   engine::reconcile INCORRECT for DAG diamonds (lazy one-hop sweep ≠ topo order).
//!   The engine is right for chains (its unit tests) and wrong the moment a node has
//!   deps at different hop distances. Fix = topo/ascending sweep (labkit's proven shape).
//! - Temporal  substrate, not graph parity (see bottom)
//! - GraphStore · storage CLOSED: collapse measured +4% at scale -> REJECTED; shape = the split
//!   two-plane pair. Epic 1 landed (stamp + measure_storage). Forward = a namespace-generic engine
//!   (`GraphNs` prefix; see `GraphStorePlan`). `Layout` is the Epic-1 measurement knob, retires
//!   when the engine threads to `GraphNs`.
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
// CLOSED 2026-07-22 (the storage question is answered, not open):
//   EDGES were already settled — cx_dep (engine.rs:119) and rx_dep (engine.rs:980)
//   are the same 2-col WITHOUT ROWID shape, both directions indexed.
//   NODES: split-vs-collapsed MEASURED — collapse is +4% at every scale that matters
//   (5.66 GB / 82M-node run; see measure::measure_storage_scaled). So the shape is
//   the SPLIT two-plane pair; no unified table. The small-corpus "collapsed wins" was
//   fixed table-overhead. The remaining node question is GRANULARITY (per-tuple vs
//   per-rel reconcile), not columns — that is the frontier in `GraphStorePlan`,
//   measured on the split shape, no collapse required.
// This trait (a putative generic node/edge API) stays aspirational; it is not what
// ships. The shipped store is `relstore::RelStore` over the split cx_/rx_ tables.
pub trait GraphStore {
    async fn create(&self, node_value_cols: &[&str], per_tuple: bool) -> ();
    async fn upsert_node(&self, key: i64, values: &[i64]) -> ();
    async fn upsert_edges(&self, edges: &[(i64, i64)]) -> ();
    async fn children(&self, key: i64) -> Vec<i64>; // forward traversal (cascade hits)
    async fn parents(&self, key: i64) -> Vec<i64>;  // reverse traversal (rederive / dirty)
}

// ─── 2026-07-22 · GraphStore: storage CLOSED; forward = a namespace-generic engine ───
// The shape is settled (the split two-plane pair; collapse measured +4% -> rejected).
// The open work is NOT a shape change: make the one shape NAMESPACE-GENERIC so the
// same engine stamps + runs multiple independent graph stores in one db. Edit this
// section in place; strike a line when its real impl lands.

// Real types the plan consumes:
//   GraphNs      = crate::relstore::GraphNs   — 14 table/index/TEMP names from a prefix
//                                               (default "" = the live cx_/rx_ set)
//   StorageDelta = crate::measure::StorageDelta — the verdict (collapse +4%)
// Mechanism = NAME PREFIX, not schema-qualify: SQLite TEMP working tables live in
// temp. and CANNOT be qualified to an ATTACH'd schema, so prefix is the only
// namespace that covers the working set. Forced, not a fork.

// Proof tokens RELEASED by an unlanded task:
pub struct Namespaced;   // Epic 2 : the engine addresses GraphNs names end-to-end
pub struct Independent;  // Epic 3 : two stores in one db retract without cross-talk
pub struct Evidence;     // frontier : a measurement that would close a question

// ── EPIC 1 · LANDED · stamp harness + storage measurement -> verdict ──
//   stamp / attach_with / measure_storage landed in relstore + measure. Scaled verdict
//   (measure_storage_scaled, `just storage L W`): collapsed/split = 1.040 at 5.66 GB
//   (82M nodes / 164M edges, +234 MB), ~1.046 stable 300K->2M nodes. Collapsed is +4%
//   at EVERY scale that matters -> REJECTED on storage. measure_storage_scaled stays as
//   the frozen evidence; `Layout` (relstore) is its measurement knob, to retire once the
//   engine threads to GraphNs. Collapse's real aim was the per-tuple-reconcile
//   granularity unlock — that is the frontier below, doable on the split shape.

// ── EPIC 2 · OPEN · thread GraphNs through the engine (namespace-generic) ──
//   stamp(db, &GraphNs) stamps cx_/rx_ (+ TEMP + indexes) under the prefix. Thread
//   &GraphNs through cascade (insert_rows/insert_deps/retract/retract_scc/retract_dred/
//   retract_dred_cte/assert), reconcile (seed/mark_changed/dirty/verify), reach
//   (reaches_from/reached_by/multi_source_walk/scc_labels/count_pairs), + algo
//   SqliteReach. RelStore owns its GraphNs. Default ns ("") = today's names, so every
//   existing test stays green throughout; a custom prefix yields an independent store.
//   Releases `Namespaced`.

// ── EPIC 3 · OPEN · proof: two independent stores in one db ──
//   Golden: stamp default + stamp "b_" in ONE db; load + retract in each; assert
//   independent survivor sets (no cross-talk). Proves the engine is namespace-generic,
//   not just the DDL. Releases `Independent`.

// ── FRONTIER (the real remaining lever; measured on the split shape, no collapse) ──
//   per-tuple reconcile. rx_memo is per-rel TODAY: the API keys id = key(rel,row)
//   (lib.rs:829-848; dirty() maps back to (rel,row) at lib.rs:838-843), but the MODEL is
//   one memo row per reactive relation (engine.rs:940) and the tests seed one id per
//   relation (engine.rs:1102-1140). The row dimension is collapsed: one digest rolls up
//   a whole relation's output.
//
//   GRANULARITY-AGNOSTIC LAYER (the load-bearing finding): the reconcile storage + query
//   layer does NOT change to go per-tuple.
//     · schema: rx_memo/rx_dep are already keyed by key(rel,row); KEY_STRIDE
//       (engine.rs:51) already splits rel from row. No DDL change.
//     · query: dirty() (engine.rs:1024) + verify() (engine.rs:1040) run on bare i64 ids;
//       they cannot tell a relation-key from a tuple-key.
//     · RelStore control API: already (rel,row) in, (rel,row) out.
//   So per-tuple is a MODEL + SEEDING-CONTRACT change + a measurement, NOT a schema or
//   query rewrite. The work is the seeding contract:
//     · seed/verify once per OUTPUT TUPLE with that tuple's own digest;
//     · deps = the actual tuples read, key(dep_rel, dep_tuple), not the relations read;
//     · mark_changed on input tuples;
//     · rx_dep goes O(rels) -> O(tuple-reads): denser, same 2-col WITHOUT ROWID shape.
//
//   THE WIN = blast radius. Today a one-input change dirties every relation downstream
//   and each re-runs whole. Per-tuple, the dirty frontier is exactly the tuples that
//   transitively READ the changed input. Worked example (3 rels, 6 tuples, 1 input
//   change: a0->b0->{c0,c1}, a1->b1->c1; trigger = a1):
//     · per-rel:   re-runs b0,b1,c0,c1 (4 tuples)  [b0,c0 had stable inputs]
//     · per-tuple: re-runs b1,c1       (2 tuples)  [b0,c0 spared]
//   Early cutoff isolates a no-op change to 1 recompute (verify(b1) not-moved -> c1 never
//   dirties); per-rel cannot isolate inside a relation.
//
//   THE MEASUREMENT (Evidence): does per-tuple early-cutoff beat per-rel on a sparse-
//   change workload, or does the denser rx_dep cost more in storage + CTE time than the
//   recomputes it spares? Same shape of question collapse settled with a 5.66 GB run, not
//   argument.
//
//   ADJACENT: the salsa oracle is still TODO (oracle.rs:2; reconcile seed/mark_changed/
//   dirty/verify carry "oracle salsa NOT ported" at tasks.rs:280-292). It is the natural
//   place to express per-tuple semantics + prove the SQLite plane matches, so it sits
//   just upstream of this frontier.

/// The remaining plan, as a trait. A method's ARGS are body predicates (facts
/// released earlier); its RETURN is the head predicate. Epic 1 released the verdict;
/// Epic 2 threads the namespace; Epic 3 proves independence.
pub trait GraphStorePlan {
    /// 2  thread GraphNs through cascade + reconcile + reach (Epic 2). Default ns
    ///    keeps every existing test green; a custom prefix yields an independent store.
    async fn thread_namespace(&self, ns: &crate::relstore::GraphNs) -> Result<Namespaced, sea_orm::DbErr>;
    /// 3  two stores (default + prefixed) in one db retract without cross-talk (Epic 3).
    async fn two_stores_independent(&self, proof: &Namespaced) -> Independent;
    /// frontier: does per-tuple reconcile beat per-rel on the split shape? the real lever.
    /// The reconcile layer is already granularity-agnostic (see FRONTIER above); this
    /// measures whether a per-tuple seeding contract cuts blast radius enough to pay for
    /// the denser rx_dep. Returns Evidence, not a shipped change.
    fn per_tuple_unlock_evidence(&self) -> Evidence;
}
// ─── end 2026-07-22 GraphStore plan ───

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
    /// ⚠ SQLite `reconcile::seed` :992 · oracle salsa ✅ ported (oracle::salsa) · engine
    ///   is correct for chains, INCORRECT for DAG diamonds — see `dirty`.
    async fn seed(&self, _id: i64, _digest: i64, _deps: &[(i64, i64)], _rev: i64) {
        todo!()
    }
    /// ⚠ SQLite `reconcile::mark_changed` :1013 · oracle ✅ · engine chains-only (see dirty).
    async fn mark_changed(&self, _ids: &[i64], _rev: i64) {
        todo!()
    }
    /// ⚠ SQLite `reconcile::dirty` :1024 · oracle ✅ · THE BUG: the lazy one-hop `dirty()`
    ///   frontier verifies nodes in HOP-distance-from-source order, which is NOT
    ///   topological order on a DAG with diamonds. A node at hop 1 (reads an edited cell)
    ///   that also reads a hop-2 dep is verified against that still-stale dep and never
    ///   re-dirtied (under one edit `rev`, `changed_at > verified_at` is false once both
    ///   equal `rev`). Proven by tests/reconcile.rs (#[ignore]'d): on n=32 the engine
    ///   recomputes exactly the right SET (missed=[]) but wrong VALUES for every node
    ///   with a greater-hop dep. Fix = topo/ascending sweep (labkit SqlReconciler shape).
    async fn dirty(&self) -> Vec<i64> {
        todo!()
    }
    /// ⚠ SQLite `reconcile::verify` :1040 · oracle ✅ · engine chains-only (see dirty).
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
