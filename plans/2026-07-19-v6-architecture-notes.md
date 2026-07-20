# v6 architecture notes (2026-07-19)

Captured from the owner during the v5 post-mortem session. These are HIS
decisions and ideas, recorded verbatim in intent. Measurements are from running
`dl` against the sprefa root. Nothing here is an agent recommendation.

## Product thesis (restated by the owner, so v6 keeps aim)

Reconstruct the IMPLICIT pointers that per-language module systems cannot see:
a bash script points at a file; that bash script is invoked by another repo's
orchestration; a test points at a fixture; a CI job points at a script. Enumerate
those manual cross-file / cross-repo / cross-revision pointers as one graph, then
run network flow and path analysis over it. Path analysis is graph algorithms.

An LSP in the browser should be able to say "this file is pointed at by this
bash script, which is called by this other repo."

The heuristic language graphs (call/type/dataflow) are DELIBERATE and stay: SCIP
is desirable but infeasible to run over 500 repos on a dev machine. The heuristics
are a cheap ratchet toward parity using the dirtiest techniques on the
highest-value features. Design intent, not an accident.

## Decision 1 — pure extraction module

Move SCIP, language parsing, AST, typegraph, and the heuristic extractors into
ONE pure module. Facts in, facts out. No db, no engine, no daemon. The engine
then holds the DSL, the fixpoint, and the reactive layer, talking to a pure fact
source across a seam.

Consequence the owner named: pure code has no tendrils, so it moves to v6 as
copy-paste, and the SCIP-parity ratchet keeps running inside that module without
touching the engine.

## Decision 2 — types.rs hierarchy (header-file discipline)

Types live in a `types.rs` at the directory level that owns them, never dumped in
`mod.rs` or `lib.rs`:

- A type used by every file under directory `D` lives in `D/types.rs`.
- A type public to the crate lives at the crate's top level, because that is what
  "public crate type" means.
- Generally: a type belongs at the SHALLOWEST directory that dominates all of its
  use sites.

That last form is a lowest-common-ancestor rule over the module tree, which makes
it mechanically checkable rather than a style preference.

**Candidate rail** (dl enforcing dl): for each type, collect its use sites, take
the common ancestor directory, compare against where it is declared, and emit a
`diag` when the declaration sits deeper or shallower than the LCA. Inputs that
already exist: `type_entity` (declaration site), `type_edge`/`scip_ref` (use
sites), and the path structure for the ancestor walk.

## Decision 3 — one reactive base layer across crates

The owner's model: SolidJS-style signals for derived state, RxJS observables when
manual event orchestration is needed, pure functions at the bottom derived from
the reactive layer at the top. A Subject plus dispatch is the same thing as a
method call, so the CLI and the HTTP server are already the same shape.

Requirement: ONE primitive across every crate. The thing to eliminate is
per-site improvisation — a channel here, a stream there, a signal here, a
mutable there, a thread-local there.

Owner's encoding note: a JS signal library's subscription graph is literally a
graph of pointers, so petgraph (or a slotmap/arena of indices) can encode it in
Rust instead of pointer chasing.

Library research commissioned per the standing build-vs-buy law, covering
reactive_graph, futures-signals, dioxus-signals, sycamore-reactive, rxrust,
salsa, tokio::sync, arc-swap/im/crossbeam, the actor crates, petgraph/slotmap as
the raw encoding, and the self-hosting question (whether the subscription graph
could be rows in the engine's own fixpoint). Results pending.

## Measured decomposition data (dl over its own module graph)

`scc(module_edge)` — cyclic groups, size > 1:

| size | representative |
|---|---|
| 147 | src/activity.rs |
| 20 | src/graph/mod.rs |
| 5 | src/embed/candle_be.rs |
| 3 | src/setup.rs |
| 3 | src/setup/manifest.rs |

The 147-member cycle by subsystem: engine 56, graph 20, rels 13, daemon 12,
jobq 5, cli 3, propose 3, parse 2, plus ~45 top-level files including `lib.rs`.

Hubs:

| module | fan-in | fan-out |
|---|---|---|
| src/lib.rs | 56 | 66 |
| src/engine/mod.rs | 55 | 40 |
| src/ast.rs | 48 | <7 |
| src/db.rs | 32 | <7 |
| src/lower.rs | 20 | <7 |

Heaviest subsystem coupling: engine→engine 117 (internal), graph→graph 41
(internal), daemon→daemon 24, rels→rels 24, engine→db.rs 21, engine→ast.rs 16,
rels→engine 14, rels→ast.rs 13.

Subsystem sizes: engine 32,060 lines, graph 12,379, daemon 4,490, rels 2,575,
parse+lower+typecheck ~4,200. Total src 89,151.

### Three suggested splits (from the numbers above)

1. `dl-lang` at the bottom — ast.rs, lower.rs, db.rs, spine.rs, typecheck.rs,
   parse/, lex.rs. High fan-in with low fan-out is the bottom-layer signature,
   and the existing traffic already flows one way into them.
2. `dl-extract` — graph/ (12,379 lines, its own closed 20-member SCC),
   scip_import, scip_setup, sg.rs, cst.rs, engine/extract/. This is Decision 1's
   pure module. Lands after `dl-lang` since it needs those types.
3. Split `lib.rs` — 66 out and 56 in, inside the 147-cycle. A module that both
   imports and is imported by everything is a candidate cycle driver.

Open measurement that would order these: re-run `scc(module_edge)` with `lib.rs`
edges removed. If the 147 fractures, the cycle is a facade artifact; if it holds,
the coupling is real.

## Query recipes that worked (warm daemon, seconds)

`dl daemon load-once <file.dl>` is the interactive loop — it runs against the
warm daemon and prints `?` results. A bare `dl file.dl` merges the whole
`.dl/*.dl` corpus instead (failure-modes class 23).

Seed a recursion instead of closing the graph; `closure(calls)` over the full
call graph times out past 2 minutes, while a seeded walk returns in seconds:

```dl
rel calls(caller: text, callee: text).
calls(caller, callee) <- call_edge(caller, callee, _).

rel reach(callee: text).
reach(callee) <- calls("sprefa::src/db.rs::method::Db.flush_syms", callee).
reach(callee) <- reach(mid), calls(mid, callee).
? reach(callee).
```

Datalog set semantics dedup a projection, so counting over a projected pair
collapses to 1. Keep the edge identity in the rel to count multiplicity:

```dl
rel edge_dir(src: text, dst: text, src_dir: text, dst_dir: text).
edge_dir(src, dst, split(src, "/", 1), split(dst, "/", 1)) <- module_edge(src, dst).
rel dir_pair(src_dir: text, dst_dir: text, n: int).
dir_pair(src_dir, dst_dir, count(dst)) <- edge_dir(src, dst, src_dir, dst_dir).
```

## Known v5 defect carried into any v6 planning

The flow panel's HTTP bridge issues `query_sql` with no root, so it lands on the
config-view db (112 rel tables, `rel_df_node` 0 rows) instead of the root db
(473 rel tables, `rel_df_node` 283,127 rows). The panel renders empty because it
is querying a database with no facts in it. Same shape as failure-modes class 23.

## Materialization control: prior art (research 2026-07-19)

### The keyword question (`rel` vs `mrel`) has three shipped answers

**LogicBlox `lang:derivationType`** is the closest match to the exact question.
Three values, per-predicate:

    lang:derivationType[`_Allocation:Insn] = "Derived".

| value | meaning |
|---|---|
| `Extensional` | EDB, stored, never derived |
| `DerivedAndStored` | IDB, materialized, incrementally maintained via delta rules |
| `Derived` | NO extent. "unfolded into the rule in which its derived-only predicate is used" |

Since 4.29: `#edb` / `#idb` / `#inlined` shorthands. Doop (a points-to analysis,
same workload shape as this engine) marks `_`-prefixed helper predicates
`Derived` precisely to avoid materializing intermediates.

**Soufflé `inline`** is the same mechanism: rule-body unfolding, relation
eliminated. Its docs: "This may lead to performance gains by re-computing
results rather than storing them." Hard constraint: a relation marked
`.input`/`.output`/`.printsize` CANNOT be inlined, and no cycle may have every
node inlined — so the keyword is unavailable exactly inside recursive SCCs,
where intermediate blowup is worst.

**Logica** has a three-level split closest to a SQL-lowering engine:
`@Ground` (persisted table) / `@With` (CTE, query-scoped) / `@NoWith` (inlined
subquery).

### Feldera's two-axis decomposition — the reframe

Feldera separates two things this engine currently conflates:

    maintained-incrementally (ALWAYS)  x  contents-retained (OPT-IN)

A non-materialized Feldera view is still incrementally computed; it streams
output deltas. What is not kept is the full contents. Their explicit warning:
"Materialized tables and views are NOT a performance optimization" — they exist
for INSPECTABILITY (ad-hoc queries only reach materialized relations).

### XSB incremental tabling — the runtime model matching "wake who cares"

    :- table p/2 as incremental.
    :- dynamic q/2 as incremental.

"When `incr_assert/1` is called, it sparks an invalidation phase in which tables
that depend on it are marked invalid. An Incremental Dependency Graph (IDG) is
used to obtain the right tables to invalidate. After invalidation, when/if a
subgoal calls an invalid table T the engine recomputes T. **If no calls are ever
made to an invalid incremental table T', T' will never incur the cost of an
update.**"

That is EAGER INVALIDATION, LAZY RECOMPUTATION — the residency law. Invalidity
is free; recomputation is demand-triggered.

XSB is also the only surveyed system exposing the demand graph programmatically
at runtime: `is_incremental_subgoal/1`, `incr_directly_depends/2`,
`get_incr_scc/[1,2]`, `incr_invalid_subgoals/1`, `incr_is_invalid/1`.
Closure rule: anything called by an `incremental` predicate must itself be
`incremental` or `opaque` (opaque = re-call it every time, store nothing).

SWI-Prolog differs usefully: if a recomputed table is UNCHANGED, that fact is
propagated so indirectly-affected tables need not re-evaluate. XSB 3.7
invalidates transitively at assert time with no such short-circuit.

### Dyna — this exact design, specified, never shipped

Eisner & Filardo, Datalog 2.0 (2011): "we can use a chart that stores memos for
some arbitrary subset of the items... We allow memos to be discarded at any
time. The natural strategy is a mixture of backward chaining (to query
unmemoized items) and forward chaining (to update memos). Pure backward and
forward chaining are the extreme cases where nothing and everything are
memoized."

Repo archived 2021-07-11. The memo pragma syntax does not exist in any reachable
source — only a TODO. Treat as a design to finish, not a system to copy.

### Noria (OSDI 2018) — closest SHIPPED partial materialization

Row-granular. Operators/views start EMPTY; missing state filled by UPQUERIES
sent backward up the dataflow for the specific keys needed. Eviction works like
a cache; eviction notices propagate downstream. Correctness hazard named in the
paper, directly relevant to a re-ticking daemon: **upquery responses do not
commute with updates**, so upqueries must be scoped to single-threaded operator
chains. ~3x base table size overhead; 5x throughput vs MySQL on Lobsters.

### DBSP — the correct math for incrementalization incl. recursion

    D(s) = s - z^-1(s)                  differentiation
    I(s) = fix a. (s + z^-1(a))         integration (mutual inverses)
    Q^D  = D . Q . I                    incrementalization
    (Q1 . Q2)^D = Q1^D . Q2^D           CHAIN RULE
    Q linear => Q^D = Q
    (lam s. fix a. T(s, z^-1(a)))^D = lam s. fix a. T^D(s, z^-1(a))   recursion

The chain rule means incrementalization reduces to structurally replacing each
operator with its incremental form. Recursion lifts to nested streams, so the
fixpoint is itself a stream computation — this sidesteps counting-fails-on-
recursion WITHOUT DRed's over-delete phase. PVLDB 16(7), 2023.

### Deletion is where scrappy dies

- **Counting** (Gupta/Mumick/Subrahmanian 1993): per-tuple derivation count,
  delete at zero. FAILS under recursion (a tuple can support its own derivation).
- **DRed**: over-delete transitively, then re-derive anything with surviving
  support. Correct under recursion, pessimistic.
- **DBSP**: handles it uniformly via multiplicities over a time lattice.

### The meta-lesson (strongest finding)

EVERY surveyed system grew its introspection surface AFTER its declaration
surface. `mz_arrangement_sharing`, Oracle `LAST_REFRESH_TYPE` + `EXPLAIN_MVIEW`,
XSB's IDG predicates, Datomic's per-query io-stats — all retrofits. The
declaration alone never told the user what actually happened, in any system
surveyed. Feldera is the only one that pre-empted it, by defining materialization
as an inspectability feature from the start.

Documented failure modes when residency is unclear: Oracle's `FORCE` refresh
silently falls back from delta to full recompute (default!); SQL Server indexed
views are silently ignored if any of seven SET options is wrong at DML time on
the base table; Differential Dataflow's slowest consumer sets everyone's memory
floor; RxJS `shareReplay({refCount:false})` is a documented permanent leak.

### Where the Rx analogy breaks (three places, all load-critical)

1. A fixpoint has NO natural unsubscribe mid-recursion. In a recursive SCC a
   relation is its own transitive consumer, so refcounting is circular by
   construction.
2. Rx cold = re-run per subscriber (linear in subscribers over one operator);
   Datalog cold = the whole SCC re-runs. `share()`'s cost model does not carry.
3. Rx has no vocabulary for "invalidated but recomputable from a delta" — which
   is exactly XSB's IDG state. **XSB, not Rx, is where the vocabulary for this
   problem lives.**

## Non-resident incremental: the answer (research 2026-07-19)

### The engine is ALREADY architected correctly in the key respect

`affected_derived` (src/engine/strata.rs) computes a least fixpoint over the
RULE graph — monotone growth over a finite rel set, so it terminates on cyclic
rule graphs by construction and returns the whole SCC:

    let mut affected = changed.clone();
    loop { let mut grew = false;
        for r in derived_rules {
            if affected.contains(&r.head.rel) { continue; }
            if r.body.iter().any(|it| matches!(it, Pos(a)|Neg(a)
                 if affected.contains(&a.rel))) {
                affected.insert(r.head.rel.clone()); grew = true; } }
        if !grew { break; } }

The resident graph is the PROGRAM graph (O(#rules), tiny). The DATA graph is
never materialized. Salsa keeps the data-dependency graph fully resident; this
engine does not. That difference is the whole ballgame.

### The one missing table

`_reldigest(rel, digest)` is the RESULT-HASH half of a verifying trace
(Build Systems a la Carte section 4.2.2). Missing is the DEPENDENCY half:

    trace_dep(key TEXT, dep_key TEXT, dep_hash BLOB)   -- index on dep_key

Then `SELECT key FROM trace_dep WHERE dep_key = ?` IS "join a table to find
who cares". BSaLC notes a verifying-trace system needs only ONE trace per key
(`Map k (Trace k v (Hash v))`), which is what makes it a table, not a log.

### Highest-leverage fix: verification currently costs a full scan

`eval_node2vec_rule` (src/engine/derive.rs) reads the entire edge list into a
Vec and blake3-folds it BEFORE it can decide to skip:

    let edges: Vec<(String,String)> = self.db.query_rows(edge, ...)?;
    let digest = blake3_edges(&edges);
    if self.load_rel_digest(&dkey)? == Some(digest) && ... { return Ok(()); }

O(|relation|) per tick to prove nothing changed. Make sidesteps this by
repurposing filesystem mtimes (BSaLC calls this out explicitly). The SQL
analogue: a maintained digest column or a per-relation change counter bumped by
the WRITER, so verification is a single row read.

### The law that already bit us

Every write path into an input MUST terminate in a trace row, or the trace
lies. The `@in(class)` port rels were EDB injected directly by the serving loop
(`inject_rpc`), bypassing every source-rule/family digest — an unattributable
EDB change. It went unnoticed only because draining the paired `@out` rel to
empty forced a full rebuild via `any_derived_empty`. Removing accidental
full-rebuild triggers removes the safety net. This is BSaLC section 6.5
(self-tracking): if the TASK can change without a key changing, the trace must
depend on the task too.

### Recursion: trace at SCC granularity

A verifying trace answers "is key k stale", which needs k's inputs older than k.
In `p :- p, e` that question is circular. Resolution: the trace's unit is the
SCC, not the tuple. Wake the whole component; let semi-naive delta iteration
handle recursion INSIDE the SQL fixpoint. The trace is consulted at the
recursion's boundary, never inside it. BSaLC section 6.6 declines cyclic
dependencies entirely ("a choice that most build systems also follow").

Deletion under recursion is the one place a persistent design pays extra
storage: either a per-tuple derivation count, or DRed's over-delete-then-
rederive pass, scoped to the SCC.

Noria's invariant 3 (descendant eviction) is the partial-state analogue: if an
entry is a hole, all descendants must be holes or have eviction notices in
flight. Applied to recursion, the safe move when staleness is unanswerable is
to evict the component and everything downstream, then refill on demand.

### Proof the trace works as SQL tables

Nix stores its trace in SQLite. `Realisations(drvPath, outputName) -> outputPath`
with `IndexRealisations on (drvPath, outputName)` IS a deep constructive trace
as an indexed relational table. Nix decides what to rebuild by hashing the
derivation closure and doing a keyed lookup, with NO resident graph. (It also
uses SQLite triggers to break self-reference cycles on delete — a small
precedent for cycles in a table-stored dependency structure.)

Bazel is the clean illustration of the split: the remote action cache is
persistent and content-addressed (one digest, one round trip); Skyframe is the
resident in-server incremental graph. Different subsystems, and the memory
lives entirely in the second.

### Salsa's own admission

From salsa's tuning doc, verbatim: "LRU evicts memoized values, NOT query keys
or dependency metadata. Input identities remain until the database is dropped."
rust-analyzer #19402: 5-6GB baseline ballooned to 22-30GB when cache priming
failed to increment the revision counter so nothing was ever collected.
rust-analyzer's architecture doc states the assumption outright: "the analyzer
keeps all this input data in memory and never does any IO... keeping everything
in memory is OK." That is precisely the assumption 500 repos rejects.

### Datomic: a useful negative result

`txReportQueue()` is a firehose of ALL transactions system-wide plus
client-side filtering. No server-side subscription matching, no registered-
interest table, no predicate index. The most famous deliberately-non-resident
Datalog system did NOT build the dispatch half.

### Closest shipped match: Noria / ReadySet

Partial state with HOLES per key; eviction notices flow forward, UPQUERIES flow
backward to refill. Measured on Lobsters (235 operators, 60 stateful):
- full state 789MB = 8x the 137MB base tables
- non-partial residue 73MB = 9% (25 unparameterized views with no suitable key)
- 91% of state evictable/recomputable
- vs DBToaster: 6.2GB against 17GB, 36%
ReadySet is live (release stable-260630, 2026-07-01) with RocksDB-backed
`PersistentState` and `evict_bytes`/`evict_keys`/`evict_random`.

### Dispatch cost, join-table vs resident graph

- SQLite triggers: O(#triggers) at PREPARE, then one `OP_Program` per row.
  Zero runtime catalog lookup.
- Resident predicate index: Le Subscribe 602 events/s at 6M subscriptions;
  SFF 3ms per message at 5M constraints; BE-Tree 0.5-64ms at 1M subs.
- Disk index (Yan & Garcia-Molina 1994): 1139 disk I/Os per document at 300k
  profiles vs 9668 brute force. At NVMe latency that is single-digit ms — the
  disk penalty is ~1 order of magnitude, not 3, and buys unbounded index size.
- Counting-style dispatch pays Theta(N) to ZERO the counter array per event
  regardless of selectivity. Tree-style (Gryphon) pays O(N^(1-lambda)) ~ sqrt(N).
- Noria's hot fraction was 60% of state at production scale but 38% at 10x
  scale: the larger the graph, the better the persistent design looks.

### TriggerMan: predicate index deliberately in DB tables

Hanson built the in-memory version (Ariel, interval skip lists) then rejected
it: "does not scale to very large numbers of rules since it may use a large
amount of main memory". TriggerMan (ICDE 1999) replaced it with expression
signatures whose constant sets have four interchangeable organizations, of
which the required ones are "non-indexed database table" and "indexed database
table with a clustered composite index". Verbatim: "Strategies 3 and 4 must be
implemented to make it feasible to process very large numbers of triggers...
they are mandatory in a scalable trigger system." The only published design
that puts the predicate index in DB tables on purpose, by the author who had
already built and discarded the resident version.

### LEAPS (the lazy RETE alternative)

RETE and TREAT are both O(wm^c) worst case; TREAT drops beta memories but still
enumerates the conflict set, which is itself O(wm^c). LEAPS computes ONE rule
instantiation per cycle using a stack of best-first search pointers with
dominant timestamps, avoiding conflict-set enumeration entirely. Space
O(max(ts)*c). Measured wasted work it avoids: TOURNEY 77% of instantiations
never fired, WEAVER 44%, WALTZ 36%. WME tests: JIG25 35,780 -> 11,113;
TOURNEY 1,107,259 -> 513,600. Gives up exact OPS5 conflict-resolution order.
No live implementation found; Drools 6+ uses PHREAK, a lazy descendant of
Doorenbos' unlinking rather than of LEAPS.
