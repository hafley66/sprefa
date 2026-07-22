# v6 — AGENTS.md (start here; stay out of everything else)

**One crate: `v6/sprefa-store`.** Labs deleted (in git). Do NOT read the dated
`plans/` or `findings/` into context — `git log`/`git show` them on demand. For
current numbers, **run a command**, do not read a report.

## just — run for info (`cd v6/sprefa-store`)

| command | tells you |
|---|---|
| `just map` | the mermaid living map of the crate |
| `just vibes` | current state / what is in flight |
| `just plan` | the next dispatchable steps |
| `just cover` | graph covering set matches the dd / salsa / rust oracles |
| `just agree` | oracle agreement matrix |
| `just perf` | golden perf sweep → `perf-runs.csv` |
| `just perf-1gb` | same sweep under the 1 GB cache gun |
| `just results` | latest perf table |

## The one pattern (do not re-derive — DECISIONS.md is the pin)

ONE semi-naive cascade: **frontier → one hop → prune → fixpoint**. The only thing
that varies is the prune test:

- **A · reconcile** (salsa red-green) → prune by **digest** (early cutoff)
- **B · retract/assert** (Z-set) → prune by **weight ≠ 0**
- **C · reach / blast** (SCC/TC) → prune by **reached**

SQLite is the ONE production engine (`src/engine.rs`). dd + salsa are **oracles
only** (`src/oracle.rs`) — the correctness ground truth + the resident-RAM speed
yardstick. Rust/CSR is used ONLY where dd/salsa can't express it either (SCC,
count_pairs). State lives on disk; RSS = page cache, Rust heap ≈ 0 (kills v5's
resident 36 GB swap). Keys are surrogate ints, never hashed strings (D1).

## Crate layout (5 src modules)

`lib.rs` Store+ingest · `spine.rs` data model · `engine.rs` the cascade ·
`measure.rs` golden harness · `oracle.rs` dd/salsa/rust oracles.

## Commit format (every commit)

```
<area>: <PASS|FAIL> — <the assertion being tested>

why:   <what we believe and the evidence for it>
check: <the command/receipt that decided it, or N/A>
```

## History — task → single-line insight (append, never rewrite)

| task / experiment | insight |
|---|---|
| E1 dense-int rowid key | single i64 `(tag,id)` key = fastest SQLite lookup; KEEP |
| E3 mmap_size=512MB · E5 cache=512MB · E6 threads · E7 deferred UPDATE | all REJECTED — no win over the baseline cascade |
| retract Big-O | O(rounds)=DAG depth, set-based per round; N+1-safe, writes flat ~1300 |
| WHY-DRED | counting is cheap but wrong on cycles → SCC nested fixpoint is the production retract, NOT DRed |
| G5 SCC | cycle-correct via nested fixpoint before publishing deltas |
| D-G1 | adopt no graph library — tarjan beats petgraph 1.16–1.43× storage-held-constant |
| D-G2 | keep v5 `scc.rs`/`walk.rs` as-is; all six callers use small rule graphs |
| D-G3 | tier on mean reachable-set size, not graph size (crossover 1.5–297 queries) |
| D-G4 | recursive CTE covers traversal; 3 limits: multi-seed loses index seek, cost tracks edges, reverse index mandatory (4000×) |
| D-G5 | SQL SCC quadratic (25min no-finish); count_pairs 4.3hr → Rust/CSR, not SQL |
| D-G6 | real max DFS depth 690 (not 1M); depth-cap 64 silently truncates flow_edge p99=79 |
| frp-lab | "FRP the edge, batch the core" holds; the break is memory, not correctness |
| reactor-lab | salsa is resident always, never touches disk — it eats the RAM budget |
| temporal-lab | append-only bitemporal on SQLite proven at 2000 revs, +20 MB RSS @ 3M facts |
| unification | salsa · SCC · dd are the SAME cascade, prune = digest/weight/reached |
| retract tombstones | `weight>0` filter already tombstones; "delete-at-0" was doc drift |
| 2026-07-22 labs fold | four lab crates → one `sprefa-store`; dd/salsa demoted to `oracle.rs` |

## Known gaps (drive these down)

- `measure.rs` golden list incomplete: cache counters stubbed `-1`, RSS monotonic
  (not per-phase), db_bytes WAL-underreports, missing stmt-count/page-faults/query-plan.
- `engine.rs` still carries the dead SQL-SCC/count_pairs (D-G5) — demote to oracle-only.
