# v4 Retraction + Fixed-Point + Caching Plan

Goal: when a dependency we read (a file, a git blob, an LSP buffer, a
fact/rule table) changes, retract exactly the previously-emitted rows
that depended on it, re-derive their replacements, and do so for
recursive rules, with RSS bounded by the affected slice, not the corpus.
No differential-dataflow. The mechanism is **stratified semi-naive
Datalog evaluation + DRed (Delete-and-Rederive) + explicit support
counts + a content-addressed memo keyed by a recorded dependency set +
a two-part cursor hash (identity vs value)**.

This is the only correct minimal set. Everything below is the build-out
of those five pieces.

---

## 0. The five primitives (why these and nothing more)

| Primitive | What it buys | Without it |
|---|---|---|
| Two-part cursor hash (`key_hash`, `val_hash`) | tells "same row, new value" from "new row" | every re-render looks like full churn; cannot diff |
| Recorded dependency set per render | exact, not guessed, invalidation | must re-run the world on any change |
| Content-addressed memo over SQLite | replay without re-running unchanged ops; constant RSS | recompute everything; RAM grows with corpus |
| Support counts (integer multiplicity) | a row derived by N paths survives until all N go | recursion either leaks rows or deletes live ones |
| DRed over a stratified semi-naive loop | correct retraction through recursion + negation | cycles never terminate; negation reads half-built relations |

DD gave you the last two for free. The 70% you measured missing is
exactly support-count arithmetic and delta-propagation. DRed is the
hand-rolled stand-in and it is provably correct for stratified Datalog.

---

## 1. Layer 1 — Type signatures

```rust
// ── Identity ────────────────────────────────────────────────────────
// All ids are blake3(...) → [u8;32], folded through the existing
// store.rs Layer-0b interner. No new id family math.

/// Anything externally mutable that an op reads.
struct SourceId([u8; 32]);            // blake3("src" ++ canonical_uri)

/// Monotone per-source counter. Bumped only by the event layer.
type SourceGen = u64;

/// Stable identity of one lowered op call in one pipe.
struct OpInstanceId([u8; 32]);        // blake3(pipe_hash ++ depth ++ static_args)

/// Identity subset of a cursor: the terms an op keys rows by.
struct KeyHash([u8; 32]);

/// Payload subset: everything that is value, not identity.
struct ValHash([u8; 32]);

/// A derived row. Stable: same input + same output ⇒ same RowId.
struct RowId([u8; 32]);               // blake3(owner ++ in_key ++ emit_ordinal)

/// Fingerprint of every (SourceId, SourceGen) read during one render.
struct DepFp([u8; 32]);

// ── Cursor split (Phase 0) ──────────────────────────────────────────
impl Cursor {
    /// Terms flagged `is_key` by the producing op. Order-independent.
    fn key_hash(&self) -> KeyHash;
    /// All non-key terms.
    fn val_hash(&self) -> ValHash;
}

/// An op declares which of its output terms are identity.
trait OperatorDef {
    fn key_terms(&self) -> &[&str] { &[] }   // default: whole cursor is key
}

// ── Dependency capture (Phase 2) ────────────────────────────────────
struct DepSet { reads: Vec<(SourceId, SourceGen)> }

impl RenderCtx {
    fn record_read(&self, s: SourceId);      // pushes (s, current_gen(s))
    fn take_deps(&self) -> DepSet;           // drained per input row
}

// ── Source generation (Phase 1) ────────────────────────────────────
trait SourceClock {
    fn current_gen(&self, s: SourceId) -> SourceGen;     // 0 if unseen
    fn bump(&self, s: SourceId) -> SourceGen;            // event layer only
}

// ── Memo (Phase 3) ─────────────────────────────────────────────────
struct MemoVal {
    out_rows: Vec<RowId>,
    out_keys: Vec<KeyHash>,
    dep_fp:   DepFp,
    computed_gen: u64,
}
trait Memo {
    /// None ⇒ miss. Some(stale=false) ⇒ replay out_rows, do not run op.
    fn probe(&self, owner: OpInstanceId, in_key: KeyHash, clock: &dyn SourceClock)
        -> Option<(MemoVal, /*stale*/ bool)>;
    fn put(&self, owner: OpInstanceId, in_key: KeyHash, deps: &DepSet, v: MemoVal);
}

// ── Support (Phase 5) ──────────────────────────────────────────────
// SUPPORT gains an integer `mult`. A row is live iff sum(mult) > 0.
trait SupportLedger {
    fn add(&self, row: RowId, by: OpInstanceId, in_key: KeyHash, dep: SourceId);
    fn drop_by(&self, owner: OpInstanceId, in_key: KeyHash) -> Vec<RowId>; // rows whose mult hit 0
}

// ── Re-render diff (Phase 4, the Phase-E hook) ─────────────────────
enum Delta { Assert(RowId, Cursor), Retract(RowId) }

fn reconcile(prior: &MemoVal, fresh: &[(RowId, KeyHash, Cursor)]) -> Vec<Delta>;

// ── Fixed point (Phase 6) ──────────────────────────────────────────
struct Stratum { rules: Vec<OpInstanceId> }      // computed from neg edges

fn stratify(graph: &RuntimeGraph) -> Vec<Stratum>;

fn eval_stratum(s: &Stratum, clock: &dyn SourceClock,
                memo: &dyn Memo, sup: &dyn SupportLedger,
                worklist: &Worklist) -> /*changed*/ bool;
```

---

## 2. Layer 2 — Pseudo-code bodies

```rust
// reconcile(): the heart of retraction-from-re-render.
//   build map prior: KeyHash -> RowId from prior memo
//   for (rid, k, cur) in fresh:
//       match prior.remove(k):
//           Some(old_rid) if old_rid == rid     -> nothing (stable)
//           Some(old_rid)                       -> Retract(old_rid), Assert(rid,cur)
//           None                                -> Assert(rid, cur)
//   for (_, leftover_rid) in prior remaining:   -> Retract(leftover_rid)
//   ( leftovers are rows that the new render no longer produces )

// Memo::probe(): validity is a gen comparison, never a re-run.
//   deps = MEMO_DEPS[(owner,in_key)]              // [SourceId]
//   if deps empty -> None (never computed)
//   stale = any( clock.current_gen(s) != stored_gen(owner,in_key,s) )
//   return (MEMO[(owner,in_key)], stale)

// owner re-render (driver, replaces expand.rs:239 Phase-E TODO):
//   for in_cursor in batch:
//       in_key = in_cursor.key_hash()
//       match memo.probe(owner, in_key, clock):
//           Some(v, false) -> replay v.out_rows downstream; continue   // HIT
//           Some(v, true ) -> prior = Some(v)                          // STALE
//           None           -> prior = None                             // COLD
//       ctx.deps.clear()
//       fresh = run op(in_cursor)                  // records reads via ctx
//       deps  = ctx.take_deps()
//       for (rid,_,_) in &fresh: sup.add(rid, owner, in_key, each dep s)
//       if let Some(p) = prior:
//           for d in reconcile(&p, &fresh):
//               match d { Retract(r) => cascade_retract(r), Assert(..) => enqueue }
//       else: enqueue all fresh
//       memo.put(owner, in_key, &deps, MemoVal{ .. })

// cascade_retract(r): DRed delete half.
//   stack=[r]
//   while r=stack.pop():
//       delete r from its sink table
//       for child in SUPPORT.rows_supported_via(r):
//           if sup.dec(child) hits 0: stack.push(child)   // over-delete
//   ( re-derive half runs naturally: stale owners are dirty,
//     the semi-naive loop re-asserts any row that still has support )

// eval_stratum(): semi-naive, terminates because each round only
// feeds the *delta* of lower relations and memo blocks re-derivation.
//   loop:
//       drained = worklist.dirty_in(s)             // RUNTIME_DIRTY rows
//       if drained empty: return changed
//       for owner in drained:
//           re-render owner over its dirty input delta only
//           clear_dirty(owner)
//       changed |= (any Assert/Retract this round)
//   ( negation is safe: stratify() puts any rule that antijoins R
//     in a stratum strictly above R, so R is complete first )
```

---

## 3. Layer 3 — Instance lifetimes

| Type | Holder | Lifetime | RSS |
|---|---|---|---|
| `SourceClock` impl | one per `RtCtx`, `Arc` | process | one `u64` per touched source, in `SOURCE_GEN` table, LRU-fronted |
| `DepSet` | `RefCell` in `RenderCtx` | one input row's render | cleared per row, never accumulates |
| `Memo` impl | one per `RtCtx`, `Arc` | process | cold in SQLite, hot in `StripedLru` (existing caps) |
| `SupportLedger` | backed by `SUPPORT_TABLE` | process | on disk; only touched rows resident |
| `MemoVal` / `Delta` | stack-local in driver loop | one batch | bounded by `batch_cap`, dropped per batch |
| `Stratum` plan | computed once per lower, cached on `RuntimeGraph` | until pipe relowered | tiny: a Vec of op ids |
| `Worklist` | `RUNTIME_DIRTY` fact table | process, persistent | survives restart; snapshot scans, no in-RAM queue |

Nothing here grows with corpus size. The three current unbounded
vectors (pending insert buffers, seen-set DashSets, `RuleMemo`
HashMap) are replaced or capped by this plan: `RuleMemo` becomes the
disk-backed `Memo`; seen-sets become `SOURCE_GEN` lookups; pending
buffers flush on stratum boundary.

---

## 4. Layer 4 — Storage layout, read/write order, uniqueness

### Tables (all SQLite, hot LRU over each, content-addressed ids)

```
SOURCE_GEN     ( source_id PK, gen )
MEMO           ( owner_op_id, in_key, PK(owner_op_id,in_key),
                 out_rows BLOB, out_keys BLOB, dep_fp, computed_gen )
MEMO_DEPS      ( owner_op_id, in_key, source_id, gen_seen,
                 PK(owner_op_id,in_key,source_id) )
SUPPORT        ( row_id, owner_op_id, in_key, dep_source_id, mult,
                 PK(row_id,owner_op_id,in_key,dep_source_id) )
RUNTIME_DIRTY  ( owner_uri_id, source_uri_id, generation, ... )   // exists
RUNTIME_NODE / RUNTIME_EDGE                                       // exists
<rule sink tables>  ( one relation per rule, keyed by row key cols )
```

`table_version` (sql.rs:316) and the ad-hoc generation column collapse
into `SOURCE_GEN`: a rule/fact table is itself a `SourceId`, so a write
to it bumps its gen the same way a file edit does. One clock for files,
buffers, and relations.

### Write order on a source change (must be this order)

1. Event layer: `clock.bump(s)` → new `SOURCE_GEN[s]`.
2. `dispatch_wake(s)`: walk `SUBSCRIBE` edges, `mark_dirty(owner, s, gen)`
   for every owner whose `MEMO_DEPS` lists `s`. (Exact, from recorded
   deps, not a broad scan.)
3. Stratified loop: lowest stratum first. Per dirty owner: probe memo →
   stale → re-render → `reconcile` → DRed delete half → memo.put.
4. Re-derive half falls out: any retracted row that still has support
   from another path gets re-asserted when its owner re-renders in the
   same sweep (semi-naive ensures the owner is dirty).
5. Quiescence when `RUNTIME_DIRTY` empty across all strata.

### Uniqueness / soundness conditions

- `RowId` is a pure function of (owner, in_key, emit ordinal). Same
  logical row ⇒ same id across runs ⇒ memo replay is exact.
- `key_hash` must be stable under value-only edits. Op authors declare
  `key_terms()`; default (whole cursor) is safe but coarse (more churn).
  This is the one place author intent matters; everything else is
  mechanical.
- `mult` invariant: a row is in its sink table iff `sum(SUPPORT.mult) > 0`
  for that `row_id`. DRed maintains this; checked by a debug assertion.
- Stratification invariant: no negation/antijoin edge points within or
  downward across a stratum. `stratify()` fails the lower at build time
  if the rule graph has an unstratifiable negative cycle (this is a
  user error, reported as a diagnostic, same shape as the invariants
  skill's antijoin checks).
- Termination: within a generation every relation is monotone under
  union; memo blocks re-derivation; therefore each stratum reaches a
  least fixed point in finite rounds. DRed's delete half is finite
  (bounded by transitively supported rows); re-derive half is the
  normal monotone loop.

---

## 5. Phased build-out (each phase shippable + a pinned test)

| Ph | Deliverable | Files | Test |
|---|---|---|---|
| 0 | `key_terms()` on `OperatorDef`; `Cursor::key_hash`/`val_hash` | `compile/lower/ops.rs`, `lib.rs` | unit: value edit keeps key_hash, changes val_hash |
| 1 | `SOURCE_GEN` table + `SourceClock`; subsume `table_version` | `store.rs`, `sql.rs`, `app.rs` | edit file → gen bumps; rule write → its table gen bumps |
| 2 | `RenderCtx.record_read` wired into `fs`/`read`/`fact`-read; `MEMO_DEPS` | `compile/lower/ops.rs`, `expand.rs`, `mounted_query.rs` | parsed file appears in its owner's MEMO_DEPS |
| 3 | `Memo` probe/put + replay path in driver | `expand.rs`, new `memo.rs` | unchanged source → op dispatch count == 0 (replay) |
| 4 | `reconcile()` at the Phase-E hook (`expand.rs:239`) | `expand.rs` | value edit → exactly 1 Retract + 1 Assert |
| 5 | `mult` column on `SUPPORT`; DRed `cascade_retract` | `mounted_query.rs`, `runtime_graph.rs` | row with 2 supports survives losing 1 |
| 6 | `stratify()` + semi-naive `eval_stratum`; recursive rule fixpoint | `rule.rs`, `runtime_graph.rs` | transitive-closure rule terminates; edit mid-graph retracts closure |
| 7 | Constant-RSS proof harness | `tests/`, `src/bin/v4_bench.rs` | 500-repo corpus, edit 1 file: RSS delta < cap, recomputed owners == affected slice |

Phase 4 is the first user-visible win (LSP diagnostics that clear on
fix). Phases 5–6 are the correctness core. Phase 7 is the proof you
asked for: a pinned bench asserting RSS does not track corpus size and
that an edit recomputes only the dependency slice.

---

## 6. Diagram A — steady-state retraction on a file change

Read left to right. Each lane is a stage; time flows rightward.

```
   EVENT LAYER            SOURCE CLOCK              DIRTY WORKLIST
   ───────────            ────────────              ──────────────

   fs-watch: b.rs   ┐
   lsp didChange    ├──►  bump(src:b.rs)      ┐
   fact write       ┘     gen 7 → gen 8       │
                                              │
                          SOURCE_GEN          ▼
                          ┌──────────────┐    walk SUBSCRIBE edges
                          │ src:b.rs  8  │    where MEMO_DEPS lists
                          │ src:a.rs  3  │──► src:b.rs
                          │ rel:imports 5│    │
                          └──────────────┘    ▼
                                              mark_dirty(
                                                owner = re`fn (NAME)`,
                                                source = b.rs,
                                                gen = 8 )
                                              │
                                              ▼
                                   RUNTIME_DIRTY
                                   ┌────────────────────────┐
                                   │ (owner_re, b.rs, 8)    │
                                   └────────────────────────┘



   STRATIFIED SWEEP  (stratum 0 first)
   ─────────────────────────────────────────────────────────────────

      pull dirty owner ──►  probe MEMO(owner_re, in_key=b.rs)
                                  │
                 ┌────────────────┴───────────────────┐
                 │                                     │
          deps all equal?                       some gen differs
          gen still 7                            stored 7 ≠ now 8
                 │                                     │
                 ▼                                     ▼
            HIT: replay                          STALE: re-run op
            out_rows, op                         ┌──────────────────┐
            dispatch = 0                         │ read b.rs (gen 8) │
            (no work)                            │ ctx.record_read   │
                                                 │ parse, match      │
                                                 └────────┬──────────┘
                                                          │ fresh rows
                                                          ▼
                                                   reconcile(prior, fresh)
                                                          │
                          ┌───────────────────────────────┼────────────────────┐
                          ▼                                ▼                     ▼
                  key gone from fresh            same key, new val        new key
                  → Retract(old_row)             → Retract(old) +         → Assert(new_row)
                          │                        Assert(new)                 │
                          ▼                                                     ▼
                  cascade_retract (DRed)                               enqueue downstream
                  ┌───────────────────────┐
                  │ dec SUPPORT.mult       │
                  │ mult>0 ? keep (another │
                  │   path still derives)  │
                  │ mult==0 ? delete row,  │
                  │   recurse to children  │
                  └───────────────────────┘
                          │
                          ▼
                  re-derive half: any owner still
                  dirty re-asserts surviving rows
                  in the same sweep
                          │
                          ▼
                  memo.put(owner_re, b.rs,
                           deps=[b.rs@8], out_rows)
                          │
                          ▼
                  clear_dirty(owner_re, b.rs, 8)



   QUIESCENCE
   ──────────
      RUNTIME_DIRTY empty across every stratum
      → sweep returns. Downstream LSP diagnostics that referenced
        the retracted rows clear automatically (their support hit 0).
```

---

## 7. Diagram B — fixed point with recursion + DRed

Rules: `reach(X,Y) :- edge(X,Y).`  `reach(X,Z) :- reach(X,Y), edge(Y,Z).`
`edge` is sourced from parsing import statements (a file dependency).

```
   STRATIFY (build time, once)
   ───────────────────────────

      edge   ── no negation ──┐
      reach  ── self-recursive├──►  Stratum 0 = { edge, reach }
                              │     (no negative edge inside)
      stale? :- dep(P), not   │
                reach(P,_)    └──►  Stratum 1 = { stale }
                                    ( reads NOT reach → must be
                                      a strictly higher stratum )



   SEMI-NAIVE EVAL of Stratum 0      (Δ = rows added this round)
   ────────────────────────────────────────────────────────────────

   round 1                round 2                round 3
   ───────                ───────                ───────
   edge from parse        Δreach ⋈ edge          Δreach ⋈ edge
   → Δreach = base        → new 2-hop rows       → ∅  (memo blocks
     edges                  added to reach          re-derivation)
        │                      │                     │
        ▼                      ▼                     ▼
   reach grows           reach grows           Δ empty → FIXPOINT
   support.add(row,             support.add           stratum 0 done
     owner=reach,                                      → run stratum 1
     dep=src:imports)                                  (stale)



   NOW: a coworker edits an import in file F  (src:F bumps gen)
   ────────────────────────────────────────────────────────────────

      dispatch_wake(src:F) → mark_dirty(owner=edge-parse, F)

      DRed DELETE half                  DRed RE-DERIVE half
      ─────────────────                 ───────────────────
      edge rows from F:                 re-parse F (gen+1)
        retract                         → fresh edge rows
        │                               → semi-naive from Δ
        ▼                                 over surviving + new
      every reach row whose             → reach rows that still
      SUPPORT chains to those             have an alternate
      edge rows:                          derivation path get
        dec mult                          re-asserted (mult back
        mult==0 → delete                  > 0); ones with no
        recurse over reach⋈edge           path stay retracted
        closure                                │
        │                                      ▼
        ▼                               stale (stratum 1) re-runs
      over-deletes the                  on the *delta* of reach:
      transitive closure                antijoin auto-clears /
      reachable through F               raises diagnostics that
                                        flipped
```

Key property visible in B: the support `mult` column is what makes a
diamond (a `reach` fact derivable by two different paths) survive the
deletion of one path. Presence-only support (today's code) would wrongly
delete it. That column is the entire 70% DD was doing for you.

---

## 8. Open decisions (need a call before Phase 4)

1. `key_terms()` default. Whole-cursor-is-key is always correct but
   maximizes churn (any value edit looks like a new row). Recommend
   per-op opt-in starting with `re`/`ast`/`json` declaring capture-name
   terms as key, match span as value.
2. DRed vs Counting algorithm for retraction. DRed is simpler and
   matches the existing support table; Counting (full provenance
   semiring) is more precise under heavy recursion but needs a richer
   support row. Recommend DRed now, leave the support schema wide
   enough to upgrade.
3. Memo eviction policy. LRU over `MEMO` is correct but a cold-evicted
   memo entry forces a recompute on next touch even if deps unchanged.
   Acceptable for constant RSS; flag if recompute storms show in
   Phase 7 bench.
