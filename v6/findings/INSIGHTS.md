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
- (2026-08-05, fable) The dedup data structure, not the indirection layer, decided
  the earlier upset. The hand-written mono held one flat `FxHashSet<Pair(u64)>` of
  ~10M entries and took **9907-10540ms** on chain@10k; the emitted version shards
  the seen set per source node (`Vec<FxHashSet<u32>>`) and takes **161-264ms** on
  the same input file, same derived count 9996213 and checksum df09b2f409f8b9a8.
  40x from one storage-layout change, at half the RSS (117MB vs 300MB). A shootout
  row measures the impl you wrote, and a hand-written entrant is a confound.
- (2026-08-05, fable) `Pair` in the retired hand mono carried a comment claiming
  "packed into one u32"; it was `Pair(u64)`, `size_of == 8`. The comment shipped
  through a full standings run without anyone checking it against the type.
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
  the oracle; its own `#[cfg(test)]` vectors (`walk.rs:116-251`) are ground truth.
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
