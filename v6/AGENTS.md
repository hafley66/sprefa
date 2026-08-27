# v6 — AGENTS.md (start here; stay out of everything else)

**One crate: `v6/sprefa-store`.** Labs deleted (in git). Do NOT read the dated
`plans/` or `findings/` into context — `git log`/`git show` them on demand. For
current numbers, **run a command**, do not read a report.

**TS port (2026-07-23):** the cascade is ALSO ported 1:1 to TS at
`v6/sprefa-store/js/` (better-sqlite3, tsgo, golden 11/11, peak RSS 141 MiB). The
rxjs lowering (the next arc) composes its RelStore knobs + reads the same SQLite.
Plan: `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md`; pin: `v6/DECISIONS.md`.

## just — run the lab (`cd v6/sprefa-store`, then `just` for the full list)

| command | does |
|---|---|
| `just check` | type-check (the 0 errors / 0 warnings bar) |
| `just test` | the full test suite |
| `just cover` | graph covering set matches the dd / salsa / rust oracles |
| `just agree` | oracle agreement matrix: oracle == counting == DRed == dd |
| `just oracle` | dd single-source reach oracle |
| `just perf` | golden perf sweep → `perf-runs.csv` (release) |
| `just perf-1gb` | same sweep under a 1 GB page cache (`DL_CACHE_KIB`) |
| `just results` | latest perf table (pretty-printed) |
| `just storage` | split-vs-collapsed on-disk bytes (the verdict run; `L W` args scale it, multi-GB, heap ≈ 0) |
| `just map` | crate map: modules / src / tests / examples |
| `just vibes` | current state: working tree + recent commits |

## The one pattern (do not re-derive — DECISIONS.md is the pin)

ONE semi-naive cascade: **frontier → one hop → prune → fixpoint**. The only thing
that varies is the prune test:

- **A · reconcile** (salsa red-green) → prune by **digest** (early cutoff)
- **B · retract/assert** (Z-set) → prune by **weight ≠ 0**
- **C · reach / blast** (SCC/TC) → prune by **reached**

SQLite is the ONE production engine (`src/engine.rs`). dd + salsa are **oracles
only** (`src/oracle.rs`) — the correctness oracle + the resident-RAM speed
yardstick. Rust/CSR is used ONLY where dd/salsa can't express it either (SCC,
count_pairs). State lives on disk; RSS = page cache, Rust heap ≈ 0 (kills v5's
resident 36 GB swap). Keys are surrogate ints, never hashed strings (D1).

## Crate layout (7 src modules)

`lib.rs` Store+ingest · `spine.rs` data model · `engine.rs` the cascade ·
`measure.rs` golden harness · `oracle.rs` dd/salsa/rust oracles ·
`algo.rs` the real `Reach` trait (parity with the oracle, production).

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
| D-G5 | SQL SCC quadratic (25min no-finish); count_pairs 4.3hr → Rust/CSR, not SQL (count_pairs since DONE `9dacb0d7`; see Known gaps) |
| D-G6 | real max DFS depth 690 (not 1M); depth-cap 64 silently truncates flow_edge p99=79 |
| frp-lab | "FRP the edge, batch the core" holds; the break is memory, not correctness |
| reactor-lab | salsa is resident always, never touches disk — it eats the RAM budget |
| temporal-lab | append-only bitemporal on SQLite proven at 2000 revs, +20 MB RSS @ 3M facts |
| unification | salsa · SCC · dd are the SAME cascade, prune = digest/weight/reached |
| retract tombstones | `weight>0` filter already tombstones; "delete-at-0" was doc drift |
| 2026-07-22 labs fold | four lab crates → one `sprefa-store`; dd/salsa demoted to `oracle.rs` |
| GraphStore Epic 1 | `Layout`+`stamp` (Split delegates VERBATIM to the two create_schema; Collapsed = g_node/g_edge) + `attach_with` + `measure_storage`; collapsed g_node carries every plane's value col (the dead-byte tax) |
| GraphStore storage verdict | scaled to 5.66 GB (82M nodes): collapsed/split = 1.040 (+234 MB); ~1.046 stable 300K→2M → collapse REJECTED on storage; shape = the split two-plane pair. The small-corpus "collapsed wins" was fixed table-overhead |
| GraphStore namespace pivot | forward = a namespace-generic engine: `GraphNs` (a table-name prefix; SQLite TEMP working tables can't be schema-qualified, so prefix is FORCED, not a fork). Epic 2 = thread `GraphNs` through cascade/reconcile/reach; `Layout` retires then. Per-tuple reconcile is the real remaining lever (frontier), on the split shape |
| TS cascade port (2026-07-23) | Rust cascade → TS verbatim at `v6/sprefa-store/js/` (better-sqlite3, tsgo, bigint digests + `.safeIntegers(true)`); golden 11/11 self-contained on ported from-scratch oracles — dd/salsa NOT ported; peak RSS 141 MiB. The rxjs lowering composes the ported RelStore knobs; groupBy/LIMIT pushes INTO SQL at the `dirty` boundary (RAM thesis). Plan + bookmarks: `v6/plans/2026-07-23-v6-rxjs-lowering-and-ts-port.md` |
| 2026-07-23 retraction ruling | "SCC nested fixpoint" was doc drift (0 hits in engine.rs; retract_scc = two-pass over-delete/rederive). Production retract = counting + two-pass + dred_cte, golden-gated; retraction CLOSED (owner) |

## Known gaps (drive these down)

- `measure.rs` golden list incomplete: cache counters stubbed `-1`, RSS monotonic
  (not per-phase), db_bytes WAL-underreports, missing stmt-count/page-faults/query-plan.
- `count_pairs` is done (condensation rework, `9dacb0d7`; byte-identical to
  `src/graph/scc.rs:103`). The remaining quadratic SQL step is SCC labeling
  inside `build_condensed` (`scc_labels`) — the next measurement target. The
  `reach` module is LIVE production (bound by `tests/covering.rs`); Rust
  `scc.rs`/`walk.rs` are its oracles. Do NOT "demote" it — D-G5 is settled.
- GraphStore storage is SETTLED (collapse rejected; see history). Open GraphStore
  work is the namespace-generic engine (thread `GraphNs` through cascade/reconcile/
  reach — Epic 2) and, beyond it, per-tuple reconcile (the granularity unlock, on the
  split shape). `Layout` is the Epic-1 measurement knob, still used by `measure.rs`.
