# Incremental / Reactive Datalog + SQL: prior-art landscape

Date: 2026-05-21. Four parallel research agents covered: (1) incremental Datalog
algorithms + Soufflé, (2) the differential-dataflow / DBSP reactive family,
(3) incremental view maintenance and reactive SQL, (4) incremental code-analysis
fact engines. Frame throughout: **our design** = embeddable, in-process engine
that extracts facts from source (ast-grep / tree-sitter) into SQLite, queries
them with recursive datalog rules (transitive closure, anti-joins), and updates
incrementally on file change (watch + content-hash + file-keyed retraction).

---

## Bottom line

1. **Recursive incremental maintenance is solved, twice, in theory.**
   - Datalog lineage: the deletion problem under recursion is handled by **DRed**
     (delete-rederive), improved by **Motik et al. (Oxford)** Backward/Forward
     and the generalized **FBF** (orders of magnitude faster than DRed when facts
     have many derivations, i.e. transitive closure). The naive **counting**
     algorithm is correct only for non-recursive views; it breaks on cycles
     because a cyclic derivation supports itself and the count never hits zero.
   - Z-set lineage: **Differential Dataflow** and its formal successor **DBSP**
     represent collections as signed-weight multisets, making insert and delete
     symmetric and maintaining recursive fixpoints (transitive closure) under
     arbitrary deltas with cost proportional to change size.

2. **Off-the-shelf reactive, recursive SQL exists only as servers.**
   **Materialize** (`WITH MUTUALLY RECURSIVE`) and **Feldera** (`DECLARE
   RECURSIVE VIEW`) both maintain transitive closure incrementally and are
   production-grade. Neither is embeddable; both are separate dataflow runtimes.

3. **Embeddable recursive IVM is not turnkey.** SQLite has no materialized views
   at all and no IVM extension. `pg_ivm` (Postgres) explicitly forbids recursive
   views. `OpenIVM` (DuckDB extension, DBSP-style) is the closest *embeddable*
   IVM but is a prototype with no recursion yet. The only embeddable path to
   *recursive* incremental maintenance is to pull in a **library and drop the SQL
   surface**: the `dbsp` Rust crate (Z-set circuits) or DDlog-style differential
   Datalog.

4. **Soufflé incremental is research-branch-only.** Stable Soufflé is batch.
   David Zhao's provenance-based incremental ("elastic") evaluation with a
   `--incremental` flag was never merged to main and is unmaintained.

5. **Closest prior art to our exact design:**
   - **Glean (Meta)** — model intent: extract typed code facts, query with a
     recursive Datalog-flavored language (Angle), update incrementally via
     **ownership sets** (label each fact with the unit/file that produced it,
     propagate through the fact dependency graph). That ownership idea *is* our
     `_prov(rel, path)` file-keyed retraction. Server (RocksDB/Thrift), not
     embeddable, but the conceptual twin.
   - **GitHub Stack Graphs** — the operational twin: file-incremental
     re-indexing with intermediate results persisted in **SQLite**. Same store,
     same change granularity (name resolution rather than datalog).
   - **DDlog** (archived) — the canonical statement of delta-in/delta-out
     incremental datalog semantics. Steal the model, not the (dead) code.
   - **IncA / DRedL** and **incremental CodeQL (VIATRA/Rete)** — proof that
     DRed-family incremental maintenance works for live program analysis.
   - **Codebase-Memory (DeusData, 2026)** — a near-twin on the extraction/store
     axis: tree-sitter into one SQLite file, file-watch + content-hash re-index,
     but Cypher/recursive-CTE instead of datalog.

6. **What this says about our engine.** The design is sound and we already
   implemented Glean's ownership mechanism (`_prov` + file-keyed delete). The
   "recompute derived wholesale" choice is a legitimate stopping point that the
   literature confirms is the simple-correct option. The two real upgrade paths,
   when wholesale recompute gets slow: (a) implement Motik **B/F** for the
   recursive deletion case on top of SQLite, or (b) adopt the **`dbsp` crate**
   as the inner incremental engine and keep SQLite as cold storage. There is no
   embeddable drop-in that gives reactive recursive SQL for free.

### Read-first list
- Motik, Nenov, Piro, Horrocks — "Maintenance of Datalog Materialisations
  Revisited" (AIJ 2019) and Backward/Forward (AAAI 2015). The deletion algorithm.
- Budiu, Chajed, McSherry, Ryzhyk, Tannen — DBSP (VLDB 2023). The Z-set model.
- Glean incremental blog (ownership sets) — glean.software/blog/incremental.
- Szabó — "Incrementalizing Production CodeQL Analyses" (FSE 2023, arXiv 2308.09660).

---

## 1. Incremental Datalog evaluation + Soufflé

Two delta families underlie everything: **semi-naive** (handles inserts, the
universal substrate, native to recursion, no deletes) and the deletion answers
**Counting** (cheap, breaks on cycles) vs **DRed** (correct under recursion,
pessimistic over-deletion). For transitive-closure + anti-join workloads this
tradeoff is the core design decision; anti-joins force stratification.

- **Soufflé (core)** — Jordan/Scholz/Subotić, CAV 2016. Compile-to-C++, dominant
  for whole-program analysis (Doop). No incremental in stable; batch only.
  github.com/souffle-lang/souffle
- **Soufflé incremental / "elastic"** — David Zhao, PPDP 2021 + thesis.
  Provenance-based incremental update, "elastic" bootstrap-vs-update switch,
  handles recursion correctly. **Research branch only, never merged to main**
  (confirmed in souffle discussion #2487); `davidwzhao/souffle` branch
  `incremental-with-provenance-eager-diffs`, unmaintained.
- **Differential Datalog (DDlog)** — Ryzhyk & Budiu, VMware, Datalog 2.0 2019.
  Compiles Datalog to Differential Dataflow; fully incremental, insert/delete
  symmetric, recursion correct and incremental (gold standard). **Archived**
  (last release Dec 2021, now vmware-archive); superseded by DBSP/Feldera.
  github.com/vmware/differential-datalog
- **Feldera / DBSP** — same core team, VLDB 2023. Z-set calculus; incrementalizes
  any query including recursion. Actively developed (0.298.0, May 2026, MIT).
  SQL engine/server, but the `dbsp` Rust crate is a low-level embeddable library.
  github.com/feldera/feldera
- **RDFox / "Maintaining Datalog Materialisations"** — Motik et al., Oxford.
  The definitive modern study of the delete problem: optimised DRed, a Counting
  variant, and **FBF** (forward/backward/forward) that avoids DRed over-deletion.
  Read this for the algorithm. RDFox itself is proprietary, RDF-shaped.
  cs.ox.ac.uk/boris.motik/pubs/mnph19maintenance-revisited.pdf
- **IncA / DRedL** — Tamás Szabó, OOPSLA 2018. DRed extended to lattice-valued
  aggregation; built on the IncQuery (Rete) engine; incremental program analysis
  reacting to source edits in ms. Closest *in intent*, but JVM/EMF-bound.
  github.com/szabta89/IncA
- **Ascent** — Sahebolamri et al., CC 2022. Datalog as a Rust proc-macro;
  semi-naive, stratified negation, lattices, BYODS. **Not incremental across
  runs** (semi-naive only saves work within one solve). Best embeddable Rust
  datalog host. github.com/s-arash/ascent
- **Crepe** — datalog proc-macro, lighter than Ascent, not incremental, less
  active. github.com/ekzhang/crepe
- **Datafrog** — McSherry, rust-lang. Minimal hand-driven semi-naive (powers
  polonius). Not incremental, very embeddable/stable, very low-level.
  github.com/rust-lang/datafrog
- **FlowLog** (2025) — Zhao/Koutris et al. Per-rule relational IR over
  Differential Dataflow with datalog-specific optimisations DDlog lacked.
  Incremental, recursion via DD. Very new research artifact. github.com/hdz284/FlowLog
- **Flix** — Madsen/Lhoták. Functional language with first-class Datalog values,
  static stratification guarantee. Batch, not incremental. Design reference.
- **Formulog** — Datalog + SMT for verification. Batch. Off-path.

**Top 3 for us:** (1) Motik's FBF/DRed line — the deletion algorithm to
implement; (2) DBSP/Feldera (+DDlog ancestor) — the cleanest correct model for
incremental recursion, `dbsp` crate reusable; (3) IncA/DRedL — the direct
prior-art proof that DRed-family maintenance works for incremental code analysis.
Practical shape: Ascent/Datafrog is the embeddable host but neither is
incremental; you implement a DRed/FBF or DBSP-style delta layer yourself, Motik
for deletion + DBSP for the data model. (Salsa-outer picks which files
re-extract; a delta loop maintains the recursive relations.)

---

## 2. Differential dataflow / DBSP / reactive streaming relational

- **Timely + Differential Dataflow (McSherry)** — foundational Rust dataflow +
  incremental layer. `(data, time, diff)` triples, arrangements (shared indexed
  state), frontiers. Recursion first-class (`iterate`/`Variable`); transitive
  closure is the canonical incrementally-maintained example. **Embeddable Rust
  library**, no server. Mature + alive (differential-dataflow 0.23.0, Apr 2026).
  Surface is a Rust combinator API, not SQL/datalog. Cost: arrangements are
  RAM-resident, memory scales with maintained collection size (the main concern
  for a whole-repo store). github.com/TimelyDataflow/differential-dataflow
- **Materialize** — streaming SQL = DD wrapped so each SQL view is a maintained
  dataflow. Recursion via `WITH MUTUALLY RECURSIVE` (not standard `WITH
  RECURSIVE`), incrementally maintained. **Server/cloud, not embeddable.** Alive
  (Series C). materialize.com
- **Feldera / DBSP** — DBSP calculus (Z-sets, circuits); differentiation/
  integration make any query incremental by construction. Recursion first-class
  (`DECLARE RECURSIVE VIEW`, auto-DISTINCT, LFP over SCC graph; allows
  non-monotone recursion). **Both** a SQL server *and* the embeddable `dbsp`
  Rust crate (Z-set/circuit API, no SQL frontend). Most actively shipping in this
  list (0.298.0, May 2026, MIT). github.com/feldera/feldera (crate crates/dbsp)
- **RisingWave** — distributed streaming DB (Rust, PG-wire). Recursive CTEs
  limited, recursion not a headline; checkpoints state to S3. Server/cluster,
  not embeddable. Alive (v2.6, Sep 2025).
- **Arroyo** — Rust streaming SQL for event/window processing; no real recursion.
  Server. **Acquired by Cloudflare (2025)**, now powers Cloudflare Pipelines.
- **Noria / ReadySet** — Gjengset thesis: **partial-state** incremental
  materialized views (keep a subset of view state, evict, re-derive on demand by
  replaying upstream — directly relevant to RAM budget, opposite of DD's
  keep-everything). Acyclic dataflow, not a closure engine. Server. **Dormant**
  (no releases since Apr 2023); read the thesis, don't depend on it.
  pdos.csail.mit.edu/papers/jfrg:thesis.pdf
- **Hydro / DFIR** (ex-Hydroflow) — Berkeley+AWS Rust dataflow framework; DFIR is
  the embeddable single-node runtime. Iteration expressible, recursive relational
  IVM not packaged. Embeddable Rust library; powers GreptimeDB's stream engine.
  Research-active (POPL 2025 "Flo"). github.com/hydro-project/hydro

**Top 3 as a reactive backend:** (1) **Feldera/`dbsp` crate** — only one that is
embeddable + MIT + supports incremental recursion + most active; (2)
**differential-dataflow + timely** — deepest, production-proven recursion,
embeddable, but Rust-combinator API and RAM-heavy arrangements; (3) **Hydro/DFIR**
— embeddable, research-frontier, with Noria's partial-state idea as the RAM-budget
borrow. Eliminated: Materialize/RisingWave/Arroyo (server/cluster); ReadySet
(dormant).

---

## 3. Incremental view maintenance + reactive SQL (recursive focus)

Theory: non-recursive IVM is solved (counting, 1993). Recursive IVM is solved on
paper twice: Motik FBF (Datalog) and DBSP (Z-sets / differential dataflow). Not
solved in any embeddable SQLite-shaped package.

- **pg_ivm** — Postgres extension, trigger-based IVM (`create_immv()`). Recursive:
  **no** (`WITH RECURSIVE` disallowed). Server. Production for its subset.
  github.com/sraoss/pg_ivm
- **Feldera / DBSP** — "only engine that evaluates full SQL incrementally,
  including recursion." `DECLARE RECURSIVE VIEW` + UNION, LFP over SCC, auto
  DISTINCT; transitive closure maintained incrementally; non-monotone recursion
  allowed (recursion still flagged experimental). Embeddable **partially** (the
  `dbsp` crate, no SQL frontend). docs.feldera.com/sql/recursion
- **Materialize** — `WITH MUTUALLY RECURSIVE`, production-ready, non-linear
  recursion. Server, not embeddable.
- **OpenIVM** (Battiston/Kathuria/Boncz, CWI, SIGMOD 2024) — SQL-to-SQL compiler
  emitting IVM as plain SQL, run via **DuckDB** (DBSP approach). Recursive:
  **no** (single-table project/filter/group + SUM/COUNT; MIN/MAX/JOIN in
  progress). **Embeddable** (DuckDB extension) — closest embeddable IVM that
  exists. Prototype. arxiv.org/abs/2404.16486
- **Epsio** — commercial IVM sidecar for PG/MySQL/MSSQL, DD-like but disk-based,
  claims recursive support. Not embeddable.
- **DBToaster** (Koch et al., EPFL) — higher-order IVM via the viewlet transform
  (recursive *finite differencing of the delta expression*, NOT recursive SQL
  views). No transitive-closure IVM. Code generator, mature research artifact.
- **Streaming SQL (Flink SQL, ksqlDB, Spark Structured Streaming)** — no
  recursion; CTEs are non-recursive sugar; recursion explicitly out of scope.
  Server/JVM cluster.
- **SQLite** — no native materialized views, no IVM extension. Community pattern
  is a real table + base-table triggers doing delete/regenerate (non-incremental;
  recursion makes triggers intractable). "Ask HN: IVM for SQLite" (2023): roll
  your own or import differential-dataflow ideas.
- **Embeddable IVM-engine libraries:** `dbsp` crate (recursive, Z-set, no SQL);
  DDlog (datalog frontend → differential dataflow, recursive by construction,
  embeddable Rust lib, but original is unmaintained → Feldera is the live
  successor); **Materialite** (vlcn/cr-sqlite author) — differential dataflow for
  JS, client-side, pairs with browser SQLite, recursion not stated.

**Verdict:** reactive recursive SQL off-the-shelf = **Materialize and Feldera,
servers only**. Embeddable = **nothing turnkey**; no SQLite IVM at all; OpenIVM
on DuckDB is the closest but no recursion. The embeddable recursive path is a
library integration (the `dbsp` crate or DDlog-style differential datalog),
abandoning the SQL surface. For our SQLite datalog-over-code: no drop-in —
either keep recomputing wholesale, or hand-roll (counting for non-recursive
joins/anti-joins, DRed or Motik B/F for the recursive closure), or adopt `dbsp`
as the incremental layer with SQLite as cold storage.

---

## 4. Incremental code-analysis fact engines (closest prior art)

No single project hits all five of {embeddable in-process, tree-sitter/ast-grep
extraction, SQLite store, recursive datalog, reactive file-watch}. The field
splits into datalog-on-code that is big/server/batch vs embeddable-incremental
extractors that drop datalog.

- **Glean (Meta)** — typed code facts, queried in **Angle** (Datalog-flavored,
  recursive). RocksDB store. Incremental via **stacked immutable DBs + ownership
  sets** (label each fact with its producing unit/file, propagate through the
  fact dependency graph; Elias-Fano encoded, ~7% overhead); re-index a unit on
  demand. **Server** (Hack/Haskell/C++, Thrift), not embeddable. Alive, used at
  Meta scale. glean.software/blog/incremental
- **CodeQL / Semmle (GitHub)** — trap files → extensional DB in a Datalog dialect;
  QL is Datalog semantics + aggregates. Production evaluator is **batch**
  (semi-naive). Incremental efforts: (a) PR scans cache full-repo DB, build DB
  only for changed code (shipped Mar 2026, coarse file-level); (b) research
  prototype reusing **VIATRA (Rete-style) incremental view maintenance** (fast
  updates, high init/memory, not production). Not embeddable.
  arxiv.org/abs/2308.09660
- **rust-analyzer + Salsa** — demand-driven incremental memoization. In-memory
  memoized query graph, no relational store, not datalog. Global revision
  counter, lazy invalidation, **early-cutoff/backdating** (recomputed value equal
  to prior → dependents not re-run), "durable" inputs tier. Embeddable Rust crate
  but a memoization engine, not a fact DB. Mature, alive.
  rust-analyzer.github.io/blog/2023/07/24/durable-incrementality.html
- **Kythe (Google)** — protobuf node/edge entry stream → combined KV store. No
  datalog; graph traversal. Per-compilation incremental, graph-merge not reactive.
  Not embeddable. Alive, low momentum.
- **GitHub Stack Graphs** — stack graphs + partial paths persisted in **SQLite**.
  No query language (path-stitching name resolution). **File-incremental**
  (re-index only changed files, reuse stored partial paths). **Embeddable** Rust
  crate (`stack-graphs`). Production at GitHub. Closest to "reactive incremental
  + SQLite" in this list. github.blog/open-source/introducing-stack-graphs
- **SCIP (Sourcegraph)** — protobuf index of string symbol IDs; format + indexers,
  not an engine. Per-file incremental. Production.
- **DOOP / cclyzer++ on Soufflé** — datalog points-to; Soufflé EDB relations,
  compile-to-C++. Stock Soufflé batch; the elastic incremental fork is research.
  Soufflé can emit a linkable C++ lib.
- **DDlog (archived)** — datalog → Rust lib over Differential Dataflow. Recursion
  + delta-in/delta-out fully incremental by construction. Embeddable. **Archived**;
  steal the model. github.com/vmware-archive/differential-datalog
- **CozoDB** — embeddable transactional relational-graph DB, CozoScript datalog
  (recursion, graph algorithms, time-travel). Backends RocksDB/**SQLite**/mem. Not
  incremental-view-maintenance; recursion is per-query bottom-up; no reactive
  file-watch. Embeddable (Rust/Python/JS/C/WASM). NOTE: separately found
  effectively dormant (deps dep-rotted, will not build on a current toolchain).
- **Ascent / Crepe / Datafrog** — embeddable Rust datalog (proc-macros / minimal
  lib). Batch fixpoint per run, not incremental, no store.
- **Codebase-Memory (DeusData, 2026)** — near-twin on extraction/store:
  tree-sitter (66 langs) into one **SQLite** file (WAL); defs/calls/imports/refs.
  **Reactive** (file-watch + XXH3 content-hash re-index of changed files only).
  Query is **Cypher-like + recursive CTEs**, not datalog; exposed as MCP tools.
  Embeddable (static C binary). Very new (Feb 2026), fast-growing.
  github.com/DeusData/codebase-memory-mcp

**Closest single prior art: Glean** (model intent) — extract typed code facts,
query recursive Datalog-flavored language, update incrementally. We differ only
in deployment (embed + SQLite vs RocksDB/Thrift server). Steal: (1) the **Angle**
language design as a reference for recursive datalog over code; (2) the
**ownership-set** mechanism = label each fact with its file and propagate through
the dependency graph so a changed file invalidates exactly its downstream facts —
which maps onto a SQLite ownership column + dependency edge table, and is exactly
our `_prov` design. Pair with **DDlog** semantics (delta-in/delta-out) for how
rule evaluation should react to insert/delete, and **Stack Graphs** for the
concrete file-incremental-in-SQLite engineering pattern.

---

---

# Round 2 deep dives (2026-05-21)

## A. The RSS-frugal path to incremental recursive maintenance

The shiny option (DBSP / differential dataflow) is wrong for a peak-RSS
obsession: it keeps **arrangements resident in RAM**, so RSS scales with total
view size. Motik B/F as-published is also resident (`I` + per-fact bit-masks).
The assemblable design that pegs RSS to the **working set** instead of the view:

**SCC-condensed transitive closure, stored on disk in SQLite, never materialized.**

The unlock: **counting breaks on cycles, but condensing cycles into SCCs first
makes counting sound on the resulting DAG.** The cycle problem we kept hitting
(self-support, the count that never reaches zero) is confined to within a
strongly-connected component and removed by condensation. So:

- Condense the graph into SCCs (Tarjan). Run the cheap **counting algorithm on
  the condensed DAG** — sound, because a DAG has no cyclic derivations.
- **Do not store the closure (E+).** Reconstruct `reaches(s,d)` on query by
  joining `scc_of` to a small `scc_reach` table. Resident state tracks SCC
  structure, not the Θ(V²) closure.
- Edge insert/delete: cross-SCC edits are first-order / SQL-maintainable cheaply
  (Dong-Su / DynFO); the only hard case, intra-SCC delete, re-runs Tarjan on
  just that one SCC's induced subgraph (bounded by SCC size).
- Borrow **Glean's ownership AND-propagation** for derived facts: a derived
  fact's owner set = intersection of its premises' owners; this is `_prov`
  extended through the derivation graph, on disk.
- Borrow **Motik B/F's exact-deletion discipline** only as a bounded recursive
  CTE check ("is there a surviving proof before I delete?") so anti-joins don't
  break — run in SQLite, not as resident bit-masks.

RSS comparison (what stays resident):

| approach | resident RAM scales with | durable | recursion |
|---|---|---|---|
| wholesale recompute | total view size (the growing closure) | trivial | yes |
| DRed | view + over-deleted superset | none | yes (over-deletes) |
| Motik B/F (as published) | **total view size** + per-fact masks | none (RDFox is RAM) | yes, cycle-sound |
| DBSP / differential dataflow | **all intermediate arrangements** | none (RAM core) | yes, highest RAM |
| Noria partial state | **working set (hot rows)** | base on disk | **no recursion** |
| **SCC-condensed on SQLite (recommended)** | **one SCC + condensed neighbourhood** | **SQLite, full** | **yes, cycle-sound** |

Schema sketch (closure is a VIEW, never a table):

```sql
CREATE TABLE calls(src INT, dst INT, file TEXT);          -- file = ownership unit
CREATE TABLE scc_of(node INT PRIMARY KEY, scc INT);
CREATE TABLE scc_edge(s_src INT, s_dst INT, PRIMARY KEY(s_src,s_dst));
CREATE TABLE scc_reach(s_src INT, s_dst INT, cnt INT, PRIMARY KEY(s_src,s_dst)); -- counting on the DAG
CREATE VIEW reaches AS
  SELECT a.node s, b.node d FROM scc_of a JOIN scc_reach r ON a.scc=r.s_src JOIN scc_of b ON b.scc=r.s_dst
  UNION
  SELECT a.node, b.node FROM scc_of a JOIN scc_of b ON a.scc=b.scc
  WHERE a.scc IN (SELECT scc FROM scc_of GROUP BY scc HAVING COUNT(*)>1);
```

Cap RSS with `PRAGMA cache_size` / `sqlite3_hard_heap_limit64`; working set per
change = one SCC + its condensed neighbourhood. Lift: ownership AND-propagation
(Glean), SCC condensation + counting-on-DAG + don't-store-E+ (IncQuery IncSCC,
Bergmann/Szabó/Varró), exact-deletion check as a bounded CTE (Motik B/F), cheap
cross-SCC/acyclic maintenance (Dong-Su/DynFO). Avoid DBSP arrangements and
resident-`I`.

Sources: Motik et al. B/F (AAAI 2015) + FBF (AIJ 2019); Bergmann et al. IncSCC
(ICGT); Gjengset Noria thesis (MIT); Budiu et al. DBSP (arXiv 2203.16684);
Dong-Su TODS + Datta et al. "Reachability is in DynFO" (arXiv 1502.07467);
Glean incremental docs; Zhao et al. elastic Soufflé (PPDP 2021).

## B. codebase-memory-mcp + the slop landscape at scale

**Verdict: real, not slop** (2,479 stars, C/C++, real arXiv 2603.27277, active
releases, and it openly admits an 83%-vs-92% answer-quality regression — slop
doesn't self-criticize). But it is **RAM-first at build**: it materializes the
whole graph in an in-memory hashmap + in-memory SQLite, then bulk-dumps to disk.
Peak RSS scales with the whole graph (kernel = 2.1M nodes / 4.9M edges); its
release history shows OOM fixes (v0.4.10), forced page reclaim `mi_collect`
(v0.5.2), an O(N²) parse fix (v0.5.6); **no RSS number is published.** Queries
do page from on-disk SQLite (WAL, durable). So: durable + content-hash
incremental like us, opposite memory strategy (whole-graph-in-RAM build vs our
one-file-at-a-time).

Who actually nails scale + bounded RSS + durability (all heavyweight or archived):

| tool | storage | in-mem vs disk | RSS story | durable | real? |
|---|---|---|---|---|---|
| codebase-memory-mcp | SQLite WAL | build = whole graph in RAM; query = paged | scales w/ graph; OOM history; no number | yes | real |
| Sourcegraph Zoekt+SCIP | mmap trigram shards | mmap, only offsets resident | best-in-class, 5x RAM-reduction work | yes | gold standard |
| Meta Glean | RocksDB/LSM | disk-resident, stacked-DB incremental | LSM by design | yes | gold standard, heavyweight |
| github/stack-graphs | SQLite | per-file subgraphs, stitched lazily | bounded by per-file granularity | yes | real but ARCHIVED 2025-09 |
| CodeGraphContext | KuzuDB embedded | embedded on-disk | unproven at scale | yes | real, unproven |
| potpie | Neo4j+Postgres+Redis | servers | heavy, not embeddable | yes | real, disqualified |
| ast-grep | none | parse one file, discard | bounded by design | no | gold standard for the RSS pattern |

**The gap we fill:** nobody in the embeddable tier combines datalog-over-code +
facts on disk + bounded peak RSS. Our measured ~133 MB over the whole kernel is
matched in the wild only by the non-embeddable heavyweights (Zoekt's mmap,
Glean's LSM) and by none of the AI-generated MCP code-graph tools. Steal: Zoekt's
mmap-with-resident-offsets storage seam; Glean's stacked-DB delta + fact dedup;
stack-graphs' per-file-subgraph-stitched-lazily pattern (closest to our
discipline, but learn from it, don't depend on it — archived).

Round-2 agent handles: RSS path `a5642452469899b89`; landscape `ae3c8fa3507066aac`.

## E1 result (2026-05-21): the derived-recompute gap, measured

Instrumented `tick()` to split source-reconcile time from derived-rebuild time.
Full Rust call graph (def/call/calls/reaches/unused) over the whole sprefa repo,
860 .rs files, cold:

| phase | cost |
|---|---|
| source reconcile (extract def + call) | 730 ms |
| derived `calls` (join) + `unused` (anti-join), no recursion | 339 ms |
| derived `reaches` (transitive closure / recursion) | 197,000 ms |

Peak RSS 111 MB. `reaches` is 99.8% of the derived cost and 270x the source.
The engine recomputes all derived wholesale on any change (`if changed { wipe;
refixpoint }`), so a one-file edit pays the full 197s `reaches` rebuild.

**Verdict / gate decision:** E4 is justified but precisely scoped. The
non-recursive derived layer (join + anti-join) is cheap (339 ms) and wholesale
recompute is the right choice for it. The entire prize is the recursive
`reaches`: incrementalize / SCC-condense only that. SCC condensation is the right
tool here because the file-scoped co-occurrence `calls` graph is dense
(over-connected), which means large SCCs, which is exactly where condensation
collapses the Theta(V^2) closure. Two independent levers reduce the `reaches`
cost: (a) a sparser, AST-resolved `calls` extraction (fewer false edges → smaller
closure), and (b) SCC-condensed maintenance (structural). The earlier guess that
"derived is cheap at single-repo scale" was right for joins/anti-joins and wrong
for recursion; the measurement corrected it and narrowed the build to one rule.

## E4 result (2026-05-21): SCC-condensed reaches, measured

Built `v5/src/bin/scc_reach.rs`: iterative Tarjan SCC + condensed-DAG reachability
over `rel_calls`, counting the full closure without materializing it. Verified
correct (running example: SCC count == naive BFS count == 16). Run over the
sprefa repo's Rust call graph:

```
nodes = 3074 functions, edges = 39658, SCCs = 2112
reaches pairs (the full closure) = 2,410,759
SCC-condensed closure computed in 1.5 ms
naive recursive fixpoint for the same closure (E1) = ~197,000 ms
```

**~130,000x faster** for the identical closure (2.4M pairs), and it never
materializes those 2.4M pairs (stores the SCC partition + condensed reach, a few
MB; reconstructs any `reaches(x,y)` on demand). Bounded RSS by construction.

**The practical punchline:** you do NOT have to abandon the simple "wipe and
recompute derived each tick" model to fix the 197s. Recompute the SCC
condensation from scratch each tick (1.5 ms) instead of the materialized closure
(197 s). Same model, ~130,000x cheaper, bounded memory. Incremental IncSCC
(only-recompute-the-affected-SCC) is a further optional optimization, not needed
for the win. Next step: wire `scc_reach` into the engine's derived step — store
`scc_of` + `scc_reach` tables, expose `reaches` as a VIEW, replace the recursive
INSERT fixpoint with the SCC computation.

Query path also proven: `scc_reach <db> reaches <fn>` answers point reachability
from the condensed form (BFS over the condensed DAG + expand SCC members), e.g.
`reaches(run)` = 942 functions in **7 microseconds**, with no closure table in
existence. So both halves are bounded-memory: compute the closure in ~1.4 ms,
answer any `reaches(x,*)` in microseconds, never materialize the 2.4M pairs.
Side note: the file-scoped `calls` heuristic over-connects into one giant SCC
(most functions share a reach set) — the live argument for a sparser AST-resolved
`calls` extraction, which compounds with the SCC structural fix.

## Agent handles (for follow-up via SendMessage)
- incremental datalog + Soufflé: `a7b2b8b8d37531ddf`
- DBSP / differential dataflow: `a9096a9201a2e285f`
- IVM / reactive SQL: `ab4533f9caa0f012c`
- incremental code-analysis engines: `ad2996b4a7fd75821`
