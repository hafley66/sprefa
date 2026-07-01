# SQLite × Graph / Datalog / Reactive Query Engines — Landscape for sprefa v5

Research date: 2026-06-02. Consumer: sprefa v5, a reactive datalog-over-code engine that
extracts facts into SQLite tables, lowers recursive datalog to a SQL fixpoint loop, hand-rolls
incremental retraction, and computes Tarjan SCC / condensation / transitive closure over a code
graph. Goal of this survey: find libraries/crates that replace hand-rolled machinery, with a bias
toward modern, Rust-native, library-shaped projects.

Bottom line up front: two references dominate the signal. **CozoDB's `Storage`/`StoreTx` trait** is
the clean relation-backend abstraction sprefa wants (a KV-with-range-scan trait already implemented
over SQLite, RocksDB, sled, in-mem). **DBSP** is the one Rust-native, embeddable, MIT/Apache
incremental engine whose Z-set-with-negative-weights model is exactly the retraction math sprefa
hand-rolls. Everything else is learn-from or not-applicable.

---

## 1. CozoDB (`cozo`) — highest-signal reference

**What it is.** A transactional relational-graph-vector database written in Rust, queried with
CozoScript (a Datalog dialect). Embeds like SQLite, also runs as a server and in WASM. Recursive
Datalog is a first-class feature; the manual states "Recursion in Datalog is much easier to express,
much more powerful, and usually runs faster than in SQL." Supports recursion through a safe subset of
aggregations, and ships built-in graph algorithms as query primitives: PageRank, shortest path
(Dijkstra variant), community detection, two-hop traversal. ([github.com/cozodb/cozo][cozo-gh],
[docs.cozodb.org][cozo-v2])

**Storage abstraction (the part sprefa should study).** v0.2 "ripped apart the storage engine code,
made a nice and minimal interface out of it" so Cozo "supports swappable storage engines."
([docs.cozodb.org][cozo-v2]) The interface is a `Storage` trait ("Swappable storage trait for Cozo's
storage engine") plus an associated `StoreTx` transaction type ("A transaction needs to guarantee
MVCC semantics for all operations"). ([docs.rs/cozo][cozo-storage]) Shape, from docs.rs:

`type Tx: StoreTx<'s>;`
`fn storage_kind(&self) -> &'static str;`
`fn transact(&'s self, write: bool) -> Result<Self::Tx>;`
`fn range_compact(&'s self, lower: &[u8], upper: &[u8]) -> Result<()>;` (no-op allowed)
`fn batch_put<'a>(&'a self, data: Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + 'a>) -> Result<()>;`

The backend is a **key-value store of binary blobs with range-scan**. Cozo defines a row-oriented
binary format with memcomparable key encoding on top; the backend "does not need to know anything
about" that format. Concrete impls: `MemStorage`, `SqliteStorage`, RocksDB (`storage-rocksdb`), sled
(`storage-sled`, experimental), TiKV (`storage-tikv`, experimental). Embedded-Rust users can supply a
custom backend. ([docs.rs/cozo][cozo-storage], [github.com/cozodb/cozo][cozo-gh])

**Incremental / retraction.** No incremental view maintenance. Cozo evaluates queries (recursive
fixpoints included) from scratch on each run; there is no materialized-view delta machinery in the
docs, and the architecture is a batch Datalog evaluator over a KV store. So Cozo does *not* solve
sprefa's retraction problem; it solves the *backend-abstraction* problem.

**Library vs platform:** Library (embeddable crate) — also runs as a server, but the crate is the
primary artifact. **License:** MPL-2.0. **Language:** Rust. **Maturity (2026):** Latest release
v0.7.6, 2023-12-11; ~1,813 commits; self-described "still very young"; not archived but release
cadence has slowed. ([github.com/cozodb/cozo][cozo-gh])

**Verdict: LEARN-FROM (storage trait), possibly REUSE.** The `Storage`/`StoreTx` trait is the
RelStore-shaped seam sprefa keeps reaching for, already implemented across SQLite/RocksDB/sled/mem.
Either model sprefa's relation backend on it directly, or — since it's a Rust crate — sit sprefa's
fact tables on Cozo storage. The Datalog evaluator and graph algorithms are reusable but couple you to
CozoScript and the from-scratch evaluation model, so they don't fix the incremental seam.

---

## 2. SQLite graph extensions and idioms

**Recursive CTE traversal (the native path).** `WITH RECURSIVE` does breadth/depth-first traversal,
transitive closure, and reachability in standard SQLite. ([sqlite.org][sqlite-forum]) Limits for
sprefa: cycles require manual visited-set tracking to avoid infinite recursion; a CTE produces a
reachability/path set, not SCC labels or a condensation — there is no native Tarjan, no SCC, no
condensation. Strongly-connected detection over a cyclic code graph is awkward to express and not what
CTEs are built for. This is exactly the wall sprefa's hand-rolled SCC loop exists to clear.

**`transitive_closure` virtual table (closure.c).** SQLite's `ext/misc/closure.c` implements a
`transitive_closure` virtual table that maintains ancestor/descendant relationships. Config params:
`tablename`, `idcolumn`, `parentcolumn`; exposed columns `id` (descendant), `root` (ancestor), `depth`.
([charlesleifer.com][closure-blog]) **Tree-structured only** — it models a single self-referential
parent column and is documented for tree/hierarchy maintenance, with O(n²) worst-case storage. No DAG
or general cyclic-graph support. Not applicable to a code graph with multiple edge kinds and cycles.

**simple-graph (dpapathanasiou).** A schema + SQL pattern (not a runtime library): nodes as JSON
objects with IDs, edges as ID pairs with optional JSON props, all in SQLite; traversal via native
recursive CTEs. Implementations in Python/Go/Julia/R/Dart/Swift. MIT. Last release v2.1.0
(2022-12-31), ~168 commits, 1.5k stars. ([github.com/dpapathanasiou/simple-graph][simple-graph])
Verdict: a packaging idiom, nothing sprefa doesn't already do.

**libSQL (Turso).** A *fork* of SQLite maintained by Turso, adding embedded replicas, remote access,
and native vector search; inherits SQLite's single-writer model. ([docs.turso.tech][libsql-docs],
[github.com/tursodatabase/libsql][libsql-gh]) No graph engine; the differentiator is vectors, not
graphs.

**Turso Database (the Rust rewrite).** Separate from libSQL: an in-process SQLite-compatible database
*rewritten in Rust* (not a fork), MIT, ~85% Rust, with Go/JS/Java/.NET/Python/WASM bindings. Native
async, vector support (exact search now; ANN indexing on roadmap), FTS via Tantivy, experimental
encryption at rest. **Status: BETA**, not yet at SQLite-level reliability, but used in production
(Turso Cloud, Kin AI, Spice.ai). No graph database features or general extension architecture
described. ([github.com/tursodatabase/turso][turso-gh]) Verdict: interesting as a future Rust-native
SQLite drop-in, but no graph story today.

**Virtual-table graph engines generally:** the SQLite vtable mechanism can host a graph engine
(closure.c is the canonical example), but there is no maintained general-purpose cyclic-graph vtable
extension worth adopting. Verdict for this whole section: **NOT-APPLICABLE / LEARN-FROM** — recursive
CTE is the native tool sprefa already lowers to; the extensions are tree-only or vector-focused.

---

## 3. Other Rust embedded graph / datalog / query engines

### Batch Datalog fixpoint libraries

**datafrog** — "A lightweight Datalog engine intended to be embedded in other Rust programs."
No runtime; you build `Iteration`/`Variable` objects and apply rules in a `while iteration.changed()`
loop to a fixpoint. Originated with Frank McSherry, now under the rust-lang org; the engine behind
polonius/borrow-checker tooling. **Batch** fixpoint (not incremental during a run). Library, dual
MIT/Apache-2.0, Rust. ([github.com/rust-lang/datafrog][datafrog-gh]) Verdict: **LEARN-FROM** — this is
essentially the leapjoin-based fixpoint sprefa hand-codes in SQL; cleanest reference for the
semi-naive loop, but in-memory only (no SQLite backing, no retraction).

**ascent** — A Datalog-like logic language embedded in Rust via macros. Supports stratified negation,
aggregation, recursion/fixpoint, user-defined **lattices** (fixed points beyond Datalog), and **BYODS**
(Bring Your Own Data Structures — relations backed by custom containers to tune algorithmic
complexity). **Batch** evaluation: iterates to a fixed point then terminates; no incremental updates
during execution. Library (proc-macro), Rust, MIT/Apache. ([s-arash.github.io/ascent][ascent],
[docs.rs/ascent][ascent-docs]) Verdict: **LEARN-FROM** — the most capable in-process Datalog crate;
BYODS is the relevant idea (a relation-storage seam), but it's compile-time macro Datalog, in-memory,
not incremental, and not SQLite-welded.

**crepe** — Datalog in Rust as a procedural macro (compile rules to a fixpoint at build time). Library,
Rust. ([crates.io keyword: datalog][crepe-kw]) Verdict: **NOT-APPLICABLE** — smaller/less featured
than ascent; same batch-macro shape.

### Static graph algorithms

**petgraph** — The standard Rust graph library: four graph representations, BFS/DFS, ~14 algorithms
including `tarjan_scc`, `kosaraju_scc`, `condensation` (collapse each SCC to a node), and `toposort`
(returns `Err` on cyclic graphs; docs steer you to the SCC algorithms for cyclic input). `tarjan_scc`
returns SCCs in reverse-topological (postorder). Library, dual MIT/Apache, Rust.
([docs.rs/petgraph][petgraph-toposort], [docs.rs/petgraph][petgraph-tarjan]) Verdict: **REUSE** —
sprefa hand-rolls Tarjan SCC + condensation; petgraph already has both, battle-tested. Drop the code
graph into a `petgraph::Graph` and call `tarjan_scc` / `condensation` / `toposort` instead of
maintaining the algorithms in SQL/Rust. The only cost is the extract-to-in-memory-graph step.

### Embedded graph databases

**KuzuDB** — Embeddable, columnar, vectorized property-graph DB; Cypher; Rust bindings (`kuzu` crate);
interop with DuckDB/Parquet/Arrow. **Archived October 2025** — maintainers announced they are building
"something new" and will no longer actively support Kuzu; a final 0.11.3 bundling most extensions was
released for migration. ([github.com/kuzudb/kuzu][kuzu-gh], [lib.rs/crates/kuzu][kuzu-librs],
[biggo.com][kuzu-archived]) Verdict: **NOT-APPLICABLE** — archived, C++ core, Cypher not Datalog.

**DuckDB + SQL/PGQ** — DuckDB is a vectorized embedded analytical SQL engine with a property-graph
(SQL/PGQ) extension and a `duckdb` Rust crate. Verdict: **NOT-APPLICABLE for the graph need** —
SQL/PGQ is pattern-matching over property graphs, not recursive Datalog or incremental SCC; adds a
heavy analytical engine. Relevant only if sprefa wanted columnar analytics.

**indradb** — A graph database in Rust, usable as a Rust library, a server, or via CLI. Library +
server. ([github.com/indradb/indradb][indradb]) Verdict: **NOT-APPLICABLE** — general property-graph
store, no Datalog/fixpoint, no incremental code-graph fit.

**oxigraph** — Rust RDF triplestore with SPARQL 1.1 support; embeddable. ([HN thread][oxigraph-hn])
Verdict: **NOT-APPLICABLE** — RDF/SPARQL, not code-graph Datalog.

**GQLite** — Embedded graph DB exposing the GQL/openCypher-style query surface (SQLite-for-graphs
positioning). Verdict: **NOT-APPLICABLE / watch** — young, query-language mismatch.

### KV backends a relation store could sit on

**redb** — Pure-Rust embedded KV store (B-tree, ACID, MVCC), zero-copy, single-file. **sled** — Pure-Rust
embedded KV store (beta, lock-free B-tree). **fjall** — Pure-Rust LSM-tree KV store. All are
library-shaped, dual MIT/Apache (typical), Rust. Verdict: **REUSE (as backend), if** sprefa adopts a
Cozo-style `Storage` trait — these are exactly the KV-with-range-scan engines such a trait abstracts
(Cozo already wraps RocksDB/sled). They give a relation store a portable, non-SQLite home if desired.

### Incremental / reactive relational query libraries (besides differential-dataflow)

**DBSP / Feldera** — see §4 (the serious candidate).

**salsa** — Incremental computation framework (the rust-analyzer engine): memoized query functions with
automatic invalidation on input change. Library, Rust, MIT/Apache. Verdict: **LEARN-FROM** — salsa
solves *demand-driven memoized recompute*, not relational delta propagation; good model for sprefa's
"what to re-run on a file edit" question, but it re-executes invalidated queries rather than computing
relational deltas, so it doesn't do incremental SCC/closure.

**timely-dataflow** — The dataflow runtime under differential-dataflow. Library, Rust, MIT. Verdict:
**NOT-APPLICABLE** — the team rejected the timely/differential stack as too heavy; timely alone is the
scheduling layer, still the same weight class.

**FlowLog** — A Datalog-on-dataflow research system. Verdict: **NOT-APPLICABLE / watch** — research
maturity, not a stable embeddable crate.

**Materialize** — Incremental-view-maintenance database built on timely/differential. A **server /
platform** (PostgreSQL wire protocol), BSL-licensed, not an embeddable Rust crate.
([materialize.com][materialize]) Verdict: **NOT-APPLICABLE** — platform, not a library; same
differential engine the team rejected.

---

## 4. DBSP / Feldera (deep dive — the incremental-engine candidate)

**What it is.** DBSP is "a computational engine for continuous analysis of changing data" — a
Rust crate implementing the DBSP formal model (Budiu, Chajed, McSherry, Ryzhyk, Tannen; VLDB 2023:
"DBSP: Automatic Incremental View Maintenance for Rich Query Languages"). You write computations as if
over a complete dataset; DBSP executes them **incrementally**, with cost proportional to the *change*,
not the total data size. Core abstractions: **Circuit** (the dataflow graph), **Stream**, **Z-set /
ZSet** (tables and their changes), and relational **operators** (map/filter/aggregate/join). The Z-set
model carries **negative weights**, so deletion/retraction is native — this is the same delta-with-
retraction algebra sprefa hand-codes. ([docs.rs/dbsp][dbsp-docs], [github.com/feldera/feldera][feldera-gh],
[ACM SIGMOD][dbsp-paper])

**Library vs platform.** Both exist in the same repo. **Feldera** is the platform (server + web
console + a SQL-to-DBSP compiler, "the only engine that can evaluate full SQL syntax and semantics
completely incrementally"). **`dbsp`** is the underlying crate, published on crates.io and usable
standalone in a Rust app without the server. ([github.com/feldera/feldera][feldera-gh],
[crates.io/dbsp][dbsp-crate]) So sprefa can take the `dbsp` crate alone and skip the platform.

**Could it replace sprefa's hand-rolled incremental retraction?** Conceptually yes — this is the
single best fit for that specific seam. DBSP's whole purpose is incremental relational computation with
retraction, expressed as a circuit of operators over Z-sets; that subsumes sprefa's manual
delta/retract loop and would give incremental joins/aggregations for free. Recursive fixpoint
(transitive closure) is expressible in DBSP's nested-circuit model. The integration cost is real:
you'd rewrite sprefa's rule lowering to *target a DBSP circuit* instead of a SQL fixpoint, and DBSP
holds its traces in its own (in-memory or persistent) batch storage — it is not SQLite-welded, so the
"facts live in SQLite tables" property would change (DBSP becomes the compute layer fed *from* SQLite,
or DBSP's own persistent traces become the store).

**Memory profile.** In-memory by default but provides "both in-memory and persistent batch and trace
implementations"; Feldera "is designed to handle datasets that exceed available RAM by spilling
efficiently to disk, taking advantage of recent advances in NVMe storage." Throughput is high
("millions of events per second on a laptop without tuning"). Dependency weight is moderate-to-heavy
(70+ direct deps including tokio, serde, arrow-adjacent crates, and Feldera-internal crates).
([docs.rs/dbsp][dbsp-docs], [github.com/feldera/feldera][feldera-gh])

**Library vs platform:** `dbsp` is a library; Feldera is the platform around it. **License:** MIT (repo
badge) / the `dbsp` crate is MIT OR Apache-2.0. **Language:** Rust (MSRV ~1.93+ on recent releases).
**Maturity:** Actively developed, frequent releases (e.g. `feldera-sqllib`/`dbsp` at 0.29x+).
([crates.io/dbsp][dbsp-crate], [crates.io/feldera-sqllib][feldera-sqllib])

**Verdict: REUSE (for the incremental engine) — highest-signal reference for the retraction seam.**
The cost is targeting a DBSP circuit instead of SQL and giving up the SQLite-welded compute model.

---

## Ranked shortlist for sprefa

| # | Project | Shape | License | Why look | Verdict |
|---|---------|-------|---------|----------|---------|
| 1 | **CozoDB `Storage`/`StoreTx` trait** | Rust crate | MPL-2.0 | The RelStore-shaped KV-with-range-scan trait sprefa wants, already implemented over SQLite/RocksDB/sled/mem. | **Learn-from / reuse the trait** |
| 2 | **DBSP (`dbsp` crate)** | Rust library | MIT / Apache-2.0 | Z-set-with-negative-weights = exactly the incremental retraction sprefa hand-rolls; embeddable without the Feldera server. | **Reuse for incremental engine** |
| 3 | **petgraph** | Rust library | MIT / Apache-2.0 | `tarjan_scc` + `condensation` + `toposort` already exist; delete sprefa's hand-rolled SCC/condensation. | **Reuse the algorithms** |
| 4 | **datafrog / ascent** | Rust libraries | MIT / Apache | Cleanest references for the semi-naive fixpoint loop (datafrog) and a relation-storage seam (ascent BYODS); in-memory, batch, not SQLite-welded. | **Learn-from** |

**The two highest-signal references, explicitly:**

- **CozoDB's storage-backend trait** (`Storage` + `StoreTx`, MVCC, KV-with-range-scan over a
  memcomparable binary format) is the model for sprefa's RelStore seam — a clean trait already proven
  across four backends in production Rust.
- **DBSP's incremental engine** (Circuit / Z-set / negative-weight operators) is the model for
  sprefa's incremental-retraction machinery — the one embeddable Rust crate whose math matches the
  hand-rolled delta loop, minus differential-dataflow's weight.

The natural split: adopt a Cozo-style `Storage` trait for the *backend* (keep SQLite as the default
impl, gain RocksDB/sled/redb optionality), lift Tarjan/condensation to petgraph, and treat DBSP as the
target if/when sprefa wants true incremental view maintenance rather than a hand-rolled retract loop.
SQLite stays the welded default; everything above is additive seams, not a rewrite.

---

## Sources

[cozo-gh]: https://github.com/cozodb/cozo "cozodb/cozo — GitHub"
[cozo-storage]: https://docs.rs/cozo "cozo — docs.rs (Storage / StoreTx traits)"
[cozo-v2]: https://docs.cozodb.org/en/latest/releases/v0.2.html "Cozo v0.2 — swappable storage engines"
[sqlite-forum]: https://sqlite.org/forum/info/456e0c07ac7c1642 "SQLite Forum — breadth-first graph traversal"
[closure-blog]: https://charlesleifer.com/blog/querying-tree-structures-in-sqlite-using-python-and-the-transitive-closure-extension/ "Charles Leifer — SQLite transitive_closure extension"
[simple-graph]: https://github.com/dpapathanasiou/simple-graph "dpapathanasiou/simple-graph"
[libsql-docs]: https://docs.turso.tech/libsql "Turso docs — libSQL"
[libsql-gh]: https://github.com/tursodatabase/libsql "tursodatabase/libsql"
[turso-gh]: https://github.com/tursodatabase/turso "tursodatabase/turso — SQLite rewrite in Rust"
[datafrog-gh]: https://github.com/rust-lang/datafrog "rust-lang/datafrog"
[ascent]: https://s-arash.github.io/ascent/ "Ascent — Logic Programming in Rust"
[ascent-docs]: https://docs.rs/ascent/latest/ascent/ "ascent — docs.rs"
[crepe-kw]: https://crates.io/keywords/datalog "crates.io — datalog keyword"
[petgraph-toposort]: https://docs.rs/petgraph/latest/petgraph/algo/fn.toposort.html "petgraph::algo::toposort"
[petgraph-tarjan]: https://docs.rs/petgraph/0.4.13/petgraph/algo/fn.tarjan_scc.html "petgraph::algo::tarjan_scc"
[kuzu-gh]: https://github.com/kuzudb/kuzu "kuzudb/kuzu (archived Oct 2025)"
[kuzu-librs]: https://lib.rs/crates/kuzu "kuzu — Rust bindings"
[kuzu-archived]: https://biggo.com/news/202510130126_KuzuDB-embedded-graph-database-archived "KuzuDB archived — BigGo News"
[indradb]: https://github.com/indradb/indradb "indradb/indradb"
[oxigraph-hn]: https://news.ycombinator.com/item?id=24845761 "Oxigraph — Hacker News"
[materialize]: https://materialize.com/docs/sql/select/recursive-ctes/ "Materialize — recursive CTEs"
[dbsp-docs]: https://docs.rs/dbsp/latest/dbsp/ "dbsp — docs.rs"
[dbsp-crate]: https://crates.io/crates/dbsp "dbsp — crates.io"
[feldera-gh]: https://github.com/feldera/feldera "feldera/feldera"
[feldera-sqllib]: https://crates.io/crates/feldera-sqllib "feldera-sqllib — crates.io"
[dbsp-paper]: https://dl.acm.org/doi/10.1145/3665252.3665271 "DBSP — ACM SIGMOD Record"
