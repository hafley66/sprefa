# v6 INSIGHTS — SQL · Rust · Graph (one living ledger)

> **Shared instruction (every agent, every level): ADD RESPONSIBLY.**
> This is a durable knowledge doc, not a scratchpad. Rules:
> 1. One insight = one bullet, stated as a **claim + the receipt** (file:line, a
>    number, or a query plan). No claim without a receipt.
> 2. **Never delete** another agent's insight. If you disprove one, append a
>    counter-bullet under it marked `↳ REVISED <date>:` with your receipt; leave
>    the original visible.
> 3. Put it in the right section (SQL / Rust / Graph). If it spans two, pick the
>    primary and cross-reference.
> 4. No opinions, no vibes, no "we should". Measured facts and reproducible
>    observations only. A number you did not measure does not go here.
> 5. Date every bullet you add: `(YYYY-MM-DD, <who>)`.

This doc is the counterpart to `HYPOTHESES.md` (untested ideas) and `DECISIONS.md`
(pinned rulings). Here: things we have **learned and verified**.

---

## 1 · SQL insights (SQLite mechanics under the RAM microscope)

- (2026-07-22, opus) Recursive CTE (`WITH RECURSIVE`) buffers its working set in a
  **transient b-tree** (the recursion queue). `UNION` dedups but still materializes;
  a full reach closure is Θ(V²) rows and WILL balloon RSS on a dense graph. This is
  why the closure-based `count_pairs`/`scc_labels` in `reach.rs` are **lab
  oracle-agreement methods on small shapes, NOT the production Big-O**. Production
  paths must avoid materializing the closure. Measure every CTE run through the ONE
  uniform probe (§Rust) — RSS of a recursive CTE is not guessable from row counts.
- (2026-07-22, opus) `cx_dep` is `(parent_key, child_key)` WITHOUT ROWID, PK-ordered
  on `parent_key` (forward walk is a prefix scan) + `ix_cx_dep_child` for the reverse
  walk (`cascade.rs:114-127`). Forward reach uses the PK; reverse reach (`reached_by`)
  MUST ride `ix_cx_dep_child` or it degrades to a full scan per hop.
- _(add SQL findings here — pragma effects, query plans, cache_size behavior, temp
  b-tree spills, index selection. Attach the `EXPLAIN QUERY PLAN` text.)_

## 2 · Rust insights (allocation, measurement, the honest footprint)

- (2026-07-22, opus) `rust_peak` (memcap high-water) is near-zero for SQLite engines
  and is **NOT the footprint** — the work is in SQLite's C-heap + page cache + disk.
  Honest footprint = RSS (`getrusage.ru_maxrss`, bytes on macOS / KiB on Linux) and
  `sqlite3_memory_highwater`. Do not headline `rust_peak` for a SQLite engine.
- (2026-07-22, opus) Measurement was copy-pasted per example (`sqlite_reach.rs` has
  its own `peak_rss_mb`/`sqlite_highwater_mb`), so no two experiments measured
  identically → comparisons were guesses. **The uniform probe (`src/measure.rs`) is
  now the ONLY sanctioned path**: same pragmas, same phase boundaries, same sensor
  set, one row per (engine×workload×scale×cache_size) into `perf-runs.sqlite`. If an
  experiment reads a sensor by hand instead of through the probe, its numbers are not
  comparable and do not go in the golden-data archive.
- (2026-07-22, codex) `sqlite3_db_status` cache counters are recorded as `-1` because
  sea-orm/sqlx does not expose the raw per-connection `sqlite3*` handle needed by
  `sqlite3_db_status` (`v6/sprefa-store/src/relstore.rs:19-48`).
- (2026-07-22, opus) FOLLOW-UP on the probe (`measure.rs`), two known inaccuracies to
  fix before trusting absolute magnitudes: (1) `db_bytes` = `fs::metadata(db).len()` on
  the main file, which under-reports in WAL mode (live pages sit in `-wal` until
  checkpoint — seed run showed `db_bytes=4096` while dbstat `table_bytes=176128`). Add
  the `-wal` size or `PRAGMA wal_checkpoint(TRUNCATE)` before sizing. `final_table_bytes`
  /`final_index_bytes` come from `dbstat` and ARE accurate. (2) the archive writer
  shells out to the `sqlite3` CLI (`Command::new("sqlite3")`), a PATH dependency; the
  per-phase sensor VALUES are still captured in-process, only the sink write shells out.
  `sqlite_hw_kb` is read via `dlsym(RTLD_DEFAULT, "sqlite3_memory_highwater")` (global,
  no handle) — sound. `insert_ms≈0` in the seed run is real: the retract engines fold
  insert into build, so the insert phase is a no-op for those cells.
- (2026-08-05, fable) exec_shootout, full ladder rerun (`v6/labs/exec_shootout/STANDINGS.md`,
  9 cases x 3 families x 3 runs): **compiler-emitted rust wins all 9**, 1.01x to 2.4x
  over the rx operator graph and 6x to 21x over the IR interpreter. Chain@10k 62.1M
  vs 34.8M derived rows/sec; layered@100k 21.4M vs 9.9M; the closest case is
  chain@1M (18.59M vs 18.42M, inside noise). Peak RSS is 1.6x to 5x lower than
  rxgraph in every case. The emitter is `v6/prolog/labs/emit_rust_shootout/emit_rust.pl`
  (`swipl -g main -t halt`), which writes `mono/src/main.rs`.
  ↳ REVISED 2026-08-05, fable: that spread was measuring the seen-set layout,
  since only mono had been rewritten to shard. With rxgraph sharded to match
  (`DistinctOp`, lib.rs:76) and interp's allocation fixed, the ladder rerun gives
  **mono 8 of 9, rxgraph taking chain@1M** (22.27M vs 19.96M). The mono margin over
  rxgraph is 1.08x to 1.52x, and over interp 6.7x to 10.3x. Any shootout row where
  one entrant got a data-structure rewrite the others did not is a layout
  measurement wearing a design label.
- (2026-08-05, fable) The dedup data structure, not the indirection layer, decided
  the earlier upset. The hand-written mono held one flat `FxHashSet<Pair(u64)>` of
  ~10M entries and took **9907-10540ms** on chain@10k; the emitted version shards
  the seen set per source node (`Vec<FxHashSet<u32>>`) and takes **161-264ms** on
  the same input file, same derived count 9996213 and checksum df09b2f409f8b9a8.
  40x from one storage-layout change, at half the RSS (117MB vs 300MB). A shootout
  row measures the impl you wrote, and a hand-written entrant is a confound.
- (2026-08-05, fable) ROOT CAUSE of that 40x, isolated by holding the derive loop
  fixed and swapping only the seen-set type (chain@10k, derived 9996213 all three):
  flat `FxHashSet<Pair(u64)>` 7135ms, flat `FxHashSet<(u32,u32)>` 249ms, sharded
  `Vec<FxHashSet<u32>>` 138ms. **fxhash over one `u64` cannot spread a packed pair.**
  FxHash's finish is a multiply, and a product's low bits depend only on the
  operands' low bits, so for `Pair((source<<32)|target)` the low bits of the hash
  carry `target` alone. hashbrown indexes buckets with those low bits: over 9M keys
  built from 3000x3000 ids, `Pair(u64)` yields **3000** distinct low-24-bit values
  against **6825320** for `(u32,u32)`, whose second write mixes `source` down via
  the rotate. The hand impl's comment called the packing "for pointer-sized
  hashing", which is exactly what broke it.
- (2026-08-05, fable) `Pair` in the retired hand mono carried a comment claiming
  "packed into one u32"; it was `Pair(u64)`, `size_of == 8`. The comment shipped
  through a full standings run without anyone checking it against the type.
- (2026-08-05, fable) **Swapping the global allocator hid an allocation-volume bug,
  and stopped paying once the bug was fixed.** exec_shootout interp, macOS aarch64,
  fixpoint ms best of 3. On the original code (`Tuple = Vec<u32>`, one heap block per
  derived row): system 2695 / mimalloc 2032 / jemalloc 2408 at chain@10k, and system
  10862 / mimalloc 7521 / jemalloc 8192 at layered@100k, so mimalloc bought 1.33x to
  1.44x. After inline `SmallVec<[u32;4]>` tuples plus a dense `Vec<Option<NodeId>>`
  binding table: system 1463 / mimalloc 1439 / jemalloc 1494 at chain@10k, and system
  4950 / mimalloc 6652 / jemalloc 4467 at layered@100k. mimalloc REGRESSES on the
  fixed code and takes peak RSS from 2.2GB to 4.1GB. Default stays the platform
  allocator; both alternatives live behind `--features mimalloc-global` /
  `jemalloc-global` for future A/B. snmalloc-rs and rpmalloc are untested here.
- (2026-08-05, fable) mono's own profile is clean, and two obvious fixes to it measured
  as nothing. chain@1M, 590 top-of-stack samples: `main` (the inlined derive loop)
  36.3%, per-source `FxHashSet<u32>` insert 29.8%, `reserve_rehash` 8.6%,
  memset/bzero 8.8%, malloc/free 11.5%. Pre-sizing the shard vector from the input
  header plus swapping two delta buffers instead of allocating one per round gave
  153ms against 151ms at chain@10k and 503ms against 501ms at chain@1M, so the change
  was reverted. `Vec` growth was already amortized; the remaining allocation belongs
  to the hash tables themselves.
- (2026-08-05, fable) Dedup layout D, one bitmap per source (`Vec<u64>`, target id as
  bit index), runs chain@10k in **37ms against the sharded hash sets' 146ms**, a 3.9x
  win in 7.5MB. It does not generalize by scale: the allocation is `nodes^2 / 8`
  bytes, which is 7.5MB at 7746 nodes and **125GB** at chain@1M's 999,989 nodes. A
  generator that picks the dedup structure from a cardinality estimate beats any
  fixed choice, and that decision belongs in the emitter rather than in the runtime.
- (2026-08-05, fable) macOS `sample <pid>` is enough to find this class of defect, no
  instrumentation build needed. interp on layered@100k, 4599 top-of-stack samples:
  malloc/free **39.5%**, hashing the `Vec`-keyed dedup set 18.3%, `match_body` (the
  thing the engine exists to exhibit) 15.2%, memcmp/memmove 11.0%, the per-row
  bindings hashmap 5.9%. After the fix, 3352 samples: malloc/free **1.3%**, dedup set
  30.1%, `match_body` 26.0%. Two `Vec` allocations per candidate row were hiding in
  the inner loop (`constraints`, `bound_here`) on top of the tuple itself.
- _(add Rust findings here — allocator behavior, memcap interactions, macOS vs Linux
  rusage quirks, sea-orm/rusqlite overhead observed.)_

## 3 · Graph-theory insights (the covering set on-disk)

- (2026-07-22, opus) v5's covering set is 7 pure functions over `adj: &[Vec<u32>]`
  in `src/graph/scc.rs` + `src/graph/walk.rs`. On-disk they run over `cx_dep`. Exact
  inclusion semantics that the agreement test enforces:
  - `reaches_from(start)` = strict descendants (path len ≥ 1); includes `start` **iff**
    start's SCC is cyclic (a path returns to it). Seed the CTE from start's OUT-
    neighbors, not start itself, and this falls out.
  - `reached_by(target)` = mirror over reversed edges.
  - `count_pairs` = |{(u,v) : u reaches v}| where reaches includes u==v **iff** u is
    in a cyclic SCC. At scale this exceeds i64 → the API returns `i128`.
  - SCC comp-ids are **order-dependent**; cross-impl agreement is on the **partition**
    (canonicalize each component to its **min member key**), never on raw ids.
- (2026-07-22, opus) `multi_source_walk` records a node once per tag at its **min BFS
  depth** (`merge(MinBy(depth))`), does not expand OUT of a `halt` node (still records
  it), and does not expand a node at depth ≥ `depth_cap` (still records it). Cycles
  terminate via the per-tag visited stamp, no depth cap required. `walk.rs:40-89` is
  the oracle; its own `#[cfg(test)]` vectors (`walk.rs:116-251`) are the oracle.
- _(add graph findings here — SCC-in-SQL approaches tried, condensation costs, where
  the recursive form diverges from the resident form and why.)_

---

## APPENDIX — FROZEN CONTRACT (do not drift; all three jobs code to THIS)

### A. `src/reach.rs` public API (graph = `cx_dep`, keys are i64)

```rust
use sea_orm::{DatabaseConnection, DbErr};

/// Forward transitive closure from `start` over cx_dep (strict; includes start iff cyclic).
pub async fn reaches_from(db: &DatabaseConnection, start: i64) -> Result<Vec<i64>, DbErr>;
/// Reverse transitive closure into `target` (rides ix_cx_dep_child).
pub async fn reached_by(db: &DatabaseConnection, target: i64) -> Result<Vec<i64>, DbErr>;

/// Multi-source min-depth BFS. `halt` = node keys that record-but-don't-expand.
/// `depth_cap` = record-but-don't-expand at depth >= cap. Result sorted, deduped,
/// one (tag,node,min_depth) per (tag,node). Byte-identical to walk::multi_source_walk.
pub async fn multi_source_walk(
    db: &DatabaseConnection,
    starts: &[(i64, i64, i64)],   // (tag, node_key, start_depth)
    halt: Option<&[i64]>,         // halt node keys; None = no stop rule
    depth_cap: Option<i64>,       // None = expand to closure
) -> Result<Vec<(i64, i64, i64)>, DbErr>;

/// halt-only, depth-agnostic special case (seed depth 0, no cap, drop depth).
pub async fn multi_source_halt_bfs(
    db: &DatabaseConnection,
    starts: &[(i64, i64)],        // (tag, node_key)
    halt: &[i64],
) -> Result<Vec<(i64, i64)>, DbErr>;

/// SCC partition as (node_key, comp_repr) where comp_repr = MIN member key of the SCC.
/// Min-member canonicalization makes it directly comparable to tarjan's partition.
pub async fn scc_labels(db: &DatabaseConnection) -> Result<Vec<(i64, i64)>, DbErr>;

/// Condensation, all comp ids expressed as min-member reps.
pub struct Condensed {
    pub comp_of: Vec<(i64, i64)>,   // (node_key, comp_repr)
    pub size:    Vec<(i64, i64)>,   // (comp_repr, member_count)
    pub cyclic:  Vec<(i64, bool)>,  // (comp_repr, is_cyclic: size>1 OR self-loop)
    pub cadj:    Vec<(i64, i64)>,   // (parent_comp_repr, child_comp_repr), deduped, no self
}
pub async fn build_condensed(db: &DatabaseConnection) -> Result<Condensed, DbErr>;

/// Reachable ordered-pair count; matches scc::count_pairs byte-for-byte. i128 (exceeds i64).
pub async fn count_pairs(db: &DatabaseConnection) -> Result<i128, DbErr>;
```

### B. node ↔ key bridge (for the agreement test)

`benchgraph::encode(tag,id) = tag*1e9 + id`. When the oracle uses `adj: Vec<Vec<u32>>`
with node index `i`, the test loads edge `(i, j)` into `cx_dep` as
`(i as i64, j as i64)` (a single-tag graph → keys ARE node indices). SCC/reach results
map back by identity. For multi-tag walk vectors, keys stay small i64 == the u32 index.

### C. uniform measurement (`src/measure.rs`) — the anti-guess rule

Every perf run goes through ONE function so the sensor set + phase boundaries are
identical across engines. No example reads `getrusage`/`sqlite3_memory_highwater`/
`sqlite3_db_status` on its own anymore.

```rust
pub struct Cell {                 // the independent variables (one OS process each)
    pub engine: &'static str, pub workload: &'static str,
    pub nodes: i64, pub edges: i64, pub cache_size_kib: i64, pub memcap_mb: u64,
}
pub struct PhaseSample {          // captured at EACH phase boundary, identically
    pub phase: &'static str,      // "build" | "insert" | "op"
    pub t_ms: f64, pub rss_kb: i64, pub sqlite_hw_kb: i64,
    pub disk_read: i64, pub disk_write: i64,          // macOS rusage_info ri_diskio_*
    pub cache_hit: i64, pub cache_miss: i64, pub cache_write: i64,  // sqlite3_db_status
}
pub struct RunRow { pub cell: Cell, pub samples: Vec<PhaseSample>,
                    pub correct: bool, pub out_hash: String, pub aborted: bool }

/// Set pragmas from `cell.cache_size_kib`, run build→insert→op, sample identically
/// at each boundary, append the row to perf-runs.sqlite. THE ONLY measured path.
pub async fn run_cell<S, O>(cell: Cell, build: S, op: O) -> RunRow /* where S,O are async closures */;
```

### D. ownership (disjoint, this wave)

| file | owner | may edit lib.rs? |
|---|---|---|
| `src/reach.rs` | job A (sonnet, "terra-role") | NO (stub already declared) |
| `src/measure.rs` + `examples/reach_perf.rs` | job B (sonnet, "luna-role") | NO |
| `tests/covering.rs` | coordinator (opus) — trust anchor | — |
| `v6/findings/INSIGHTS.md` §1-3, `v6/MAP.md` DONE table | job C (haiku, grunt) | — |
