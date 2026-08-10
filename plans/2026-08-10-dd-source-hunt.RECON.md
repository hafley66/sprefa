# Differential Dataflow Retraction Source Hunt

## Context

The measured comparison is one root retraction from the same generated reachability graph. Setup is outside the timer, each child reports input and output hashes, and the Differential Dataflow implementation removes the root at logical time 1 before driving the probe to completion (`v6/sprefa-store/examples/perf_report.rs:5-16`, `v6/sprefa-store/examples/perf_report.rs:288-377`).

The checked dependency is `differential-dataflow = "0.25"` with `timely = "0.31"`; the crate lock resolves them to Differential Dataflow 0.25.1 and Timely 0.31.0 (`v6/sprefa-store/Cargo.toml:28-34`, `v6/sprefa-store/Cargo.lock:1194-1195`, `v6/sprefa-store/Cargo.lock:4116-4117`). Registry citations below therefore refer to:

```text
~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1
```

The existing report records the same gap at both requested scales. DAG 960k is 1,997.7 ms for `sqlite-count-scc`, 2,054.4 ms for `sqlite-dred-loop`, and 172.1 ms for DD. CYC 11.5M is 28,373.9 ms, 30,246.3 ms, and 2,963.1 ms respectively (`v6/sprefa-store/PERF-REPORT.md:37-49`, `v6/sprefa-store/PERF-REPORT.md:143-154`). The acyclic 11.5M row shows the same scaling shape at 26,241.4 ms, 27,402.7 ms, and 2,590.8 ms (`v6/sprefa-store/PERF-REPORT.md:77-88`).

## Matched RAM Probe

One opt-in benchmark flag was added. `DL_SQLITE_RAM_PROBE=1` selects `sqlite::memory:` and raises the SQLite page-cache ceiling to 1 GiB after the ordinary schema setup. The default file-backed path is unchanged (`v6/sprefa-store/examples/perf_report.rs:95-126`). The result record now names `storage=file` or `storage=memory` (`v6/sprefa-store/examples/perf_report.rs:490-504`).

Command shape, using the same release binary and DAG 960k input:

```text
target/release/examples/perf_report <engine> 6 160000 0
DL_SQLITE_RAM_PROBE=1 target/release/examples/perf_report <engine> 6 160000 0
```

| engine | file ms | memory ms | reduction | memory / DD | survivors | input hash | output hash |
|---|---:|---:|---:|---:|---:|---|---|
| sqlite-count-scc | 1761.441 | 1705.019 | 3.203% | 9.86x | 800002 | `ef153ee39296ef0f` | `dd6a707234617ea5` |
| sqlite-dred-loop | 1808.060 | 1697.397 | 6.120% | 9.82x | 800002 | `ef153ee39296ef0f` | `dd6a707234617ea5` |
| dd | 172.923 | n/a | n/a | 1.00x | 800002 | `ef153ee39296ef0f` | `dd6a707234617ea5` |

The memory runs reported `db_mb=0.00`. Their SQLite high-water values were 97.32 MB and their process RSS values were 423.3 MB and 422.3 MB. The DD run reported 187.38 MB of resident Rust state and 471.7 MB process RSS. The result hashes and cardinalities match across all five runs.

The store schema already places frontier, next, hits, cone, and dormant SCC tables in SQLite TEMP storage with `temp_store=MEMORY`; the persistent corpus uses a rowid table and a `WITHOUT ROWID` dependency table (`v6/sprefa-store/src/engine.rs:100-157`). The RAM probe therefore isolates the persistent database file and page-cache path. It leaves SQLite's virtual machine, B-tree maintenance, and row materialization in place.

## Source Decomposition

### Differential Dataflow update path

1. `InputSession::remove` represents the root deletion as weight `-1`. Input updates are buffered and shipped in batches when flushed (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/input.rs:128-134`, `:178-182`, `:216-251`). The benchmark performs exactly that removal and advances the outer time from 1 to 2 (`v6/sprefa-store/examples/perf_report.rs:344-356`).

2. `iterate` constructs an iterative Timely subgraph with `Product<outer_time, inner_round>` timestamps. Differences circulate through feedback until they dissipate, and the `Variable::new_from` construction subtracts the source before connecting the feedback loop (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/iterate.rs:1-15`, `:81-99`, `:227-269`). The benchmark loop is `edges.semijoin(reach).map(child).concat(roots).distinct()` (`v6/sprefa-store/examples/perf_report.rs:313-332`).

3. `semijoin` arranges the edge collection by parent key and the reach collection by node key, then invokes the arranged join (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/collection.rs:1063-1099`, `:1183-1188`).

4. The arranged join treats each newly arriving batch from one input against accepted trace history from the other input, then performs the symmetric half for the other new input. Trace histories are compacted against logical and physical frontiers, and cursor work seeks matching ordered keys (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/join.rs:178-278`, `:341-377`, `:391-467`, `:503-566`). Output multiplicities are products of input multiplicities and flow through a consolidating builder (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/arrange/arrangement.rs:230-260`).

5. Arrangement input is accumulated into sorted batches as frontiers advance. The arrange operator seals batches and inserts them into the trace (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/arrange/arrangement.rs:347-482`). The merge batcher combines sorted chains geometrically and removes equal `(data, time)` updates whose summed difference is zero (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/trace/implementations/merge_batcher.rs:61-127`, `:277-339`).

6. The trace spine stores immutable batches by size layer and performs fueled, proportional merges rather than rewriting the whole trace for each update (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/trace/implementations/spine_fueled.rs:1-28`, `:267-285`, `:365-439`, `:557-619`). Generic consolidation sorts equal records, sums differences, and drops zero totals (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/consolidation.rs:18-36`, `:88-144`).

7. `distinct` is implemented as thresholded reduction over an arrangement (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/collection.rs:827-887`). Reduction drives computation from novel batch keys and pending interesting times, reconstructs only those input and output histories, consolidates them, and emits the resulting changes (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/reduce.rs:317-497`, `:717-757`). A node consequently emits a membership change when its accumulated support crosses the distinct threshold.

### SQLite update path

`sqlite-count-scc` calls `retract_scc`, which immediately delegates to `retract_scc_two_pass` (`v6/sprefa-store/examples/perf_report.rs:243-256`, `v6/sprefa-store/src/engine.rs:330-349`). Pass one clears and refills primary-key frontier tables, traverses the dependency index, sets reachable row weights to zero, and records the cone (`v6/sprefa-store/src/engine.rs:350-404`). Pass two scans for externally supported cone rows, restores their weights, and performs another frontier traversal through the cone (`v6/sprefa-store/src/engine.rs:406-461`).

`sqlite-dred-loop` has the same two logical passes with more SQL calls. Its over-delete loop clears and fills the next frontier, updates row weights, and inserts cone membership (`v6/sprefa-store/src/engine.rs:543-625`). Its rederive loop clears and fills frontier tables again and restores weights for supported cone nodes (`v6/sprefa-store/src/engine.rs:627-690`).

Both loops drive joins from a frontier primary-key table into the indexed dependency and row tables. The SQL is delta-frontier driven rather than a full relation join (`v6/sprefa-store/src/engine.rs:370-404`, `v6/sprefa-store/src/engine.rs:557-625`, `v6/sprefa-store/src/engine.rs:658-688`). Each round still performs `DELETE`, `INSERT OR IGNORE`, `UPDATE`, and scalar count operations over SQLite tables.

### Timed phase split

An opt-in existing per-statement trace was applied to the in-memory `sqlite-count-scc` run (`v6/sprefa-store/src/engine.rs:62-75`). Excluding untimed setup statements:

| component | traced ms | share of 1682.317 ms |
|---|---:|---:|
| over-delete initialization and rounds | 871.04 | 51.8% |
| rederive base and rounds | 807.70 | 48.0% |
| trace bookkeeping and unclassified remainder | 3.58 | 0.2% |

The logged SQL durations sum to 1678.74 ms, or 99.79% of the traced wall time. This trace run is separate from the untraced 1705.019 ms RAM result. It produced the same survivor count and hashes.

## Suspect Verdicts

| suspect from brief | verdict | source result |
|---|---|---|
| (a) DRed does two passes while DD circulates signed deltas | CONFIRMED | The two SQLite passes are explicit (`v6/sprefa-store/src/engine.rs:350-461`, `:543-690`). DD enters `-1`, propagates weighted differences through timestamped feedback, consolidates cancellation, and thresholds distinct membership (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/input.rs:178-182`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/iterate.rs:81-99`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/consolidation.rs:18-36`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/reduce.rs:717-757`). DD may circulate a node at multiple inner rounds on cyclic paths. It avoids materializing an overdeleted cone followed by a second rederive walk. |
| (b) SQLite rewrites B-trees while DD appends and consolidates batches | CONFIRMED | SQLite's frontier/cone/row mutations are visible in both loops (`v6/sprefa-store/src/engine.rs:350-461`, `:543-690`). DD seals sorted batches into a spine and merges them with bounded fuel (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/arrange/arrangement.rs:415-482`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/trace/implementations/spine_fueled.rs:365-439`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/trace/implementations/merge_batcher.rs:277-339`). The RAM probe limits the file and page-cache contribution to 3.203% or 6.120% in these runs. |
| (c) SQLite reruns full joins while DD joins delta batches against traces | KILLED for this benchmark | DD does fresh-batch against accepted-history half joins (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/join.rs:178-278`). The measured SQLite SQL already uses a small frontier and indexed dependency lookup each round (`v6/sprefa-store/src/engine.rs:375-388`, `:589-603`, `:658-674`). General multi-input rules may still need a half-join schedule. |
| (d) SQLite pays SCC or stratification overhead while DD uses timestamps | KILLED for this benchmark | `retract_scc` delegates directly to the two-pass implementation and its function comment states that no SCC partition is resident (`v6/sprefa-store/src/engine.rs:330-349`). A repository search finds the `scc_*` identifiers only in schema/name declarations (`v6/sprefa-store/src/engine.rs:134-153`). DD does use product timestamps for recursion (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/operators/iterate.rs:35-41`, `:81-99`), but the measured SQLite path contains no separate SCC decomposition to remove. |

The remaining representation boundary is visible in the probe: moving SQLite's persistent corpus to memory saves 56.422 ms for count-scc and 110.663 ms for dred-loop, while 1.51 to 1.53 seconds remains between the in-memory SQLite paths and DD. SQLite continues to execute table mutations and virtual-machine instructions. DD keeps ordered resident arrangements and processes typed update batches. This representation cost overlaps with the algorithmic two-pass cost and cannot be isolated by subtracting the phase totals.

## Ranked Transfer Forks

### 1. Timestamped signed-delta fixed point

Estimated bound: the measured rederive pass is 807.70 ms, 48.0% of the traced in-memory count-scc run. Removing only that pass yields an arithmetic floor near 874.6 ms, about 5.1 times the observed DD run. Signed threshold deltas can also reduce work in the first pass, so 807.70 ms is a measured component rather than a complete projected win.

Type shape:

```text
retract_delta(store, outer_time, roots: [(Key, Diff)])
  -> (changed_reach: [(Key, Diff)], inner_rounds: u64)
```

Instance timeline and lifetime:

```text
outer time T receives root -1
  inner round 0 joins root delta with stable edges
  inner round N joins only reach delta N with stable edges
  threshold state emits -1 or +1 only when support crosses zero
  empty next delta closes T
stable support and arrangements survive into T+1
inner frontiers live only for T
```

Storage and sequence:

```sql
-- Persistent: reach_support(key primary key, support), edge(parent, child).
-- Per outer time: delta(round, key, diff), consolidated by (round, key).
-- Read current round once, join through edge(parent), group by child.
-- Add root changes, update support, emit threshold crossings into round + 1.
-- Stop when round + 1 is empty, then retain support for the next outer time.
```

Current `dd_plan` has signed arrangements, join and iterate operators, delta wires, and iterate/consolidate phases (`v6/prolog/compile/6_emit_dd_plan.pl:99-108`, `:124-147`, `:251-299`, `:359-363`, `:372-426`). Its iterate term contains only the head relation and it rejects mutual recursion (`v6/prolog/compile/6_emit_dd_plan.pl:262-285`, `:359-363`). The fork is:

1. Treat nested timestamps, feedback frontiers, and threshold history as a runtime convention behind the existing iterate operator.
2. Add an explicit IR notion for iterative scope, inner timestamp, feedback edge, and threshold state.

The transfer needs one of those choices before implementation.

### 2. Immutable epoch batches with consolidation

Estimated bound: this mechanism overlaps both measured phases. The two phases account for 99.79% of wall time, so no additive estimate can be separated from fork 1 with the present trace.

Type shape:

```text
append_batch(arrangement, epoch, updates: [(Key, Value, Time, Diff)])
  -> BatchHandle
compact(arrangement, frontier) -> CompactionReceipt
```

Instance timeline and lifetime:

```text
one immutable batch per sealed frontier
  small batches merge into size-tiered batches
  equal records consolidate and zero totals disappear
  batches before the compaction frontier may advance their times
arrangement lifetime spans outer updates
merge tasks live until their fuel is consumed
```

SQLite-shaped storage sketch:

```sql
-- Append update rows tagged by epoch instead of clearing and refilling one PK table.
-- Read stable base plus sealed update batches.
-- Consolidate periodically with GROUP BY key HAVING SUM(diff) <> 0.
-- Replace compacted epochs atomically after readers release them.
```

Current `dd_plan` names signed arrangements and their key/value columns (`v6/prolog/compile/6_emit_dd_plan.pl:121-147`, `:181-204`). Backend-wide batching and compaction can remain physical runtime policy. Per-arrangement policies, explicit batch ownership, or compaction-frontier dependencies require a new physical-plan notion.

### 3. Arranged half-join scheduling for general rules

Estimated current reach benchmark win: 0 ms attributable to join scheduling, because the SQLite reach loops are already frontier-index joins (`v6/sprefa-store/src/engine.rs:375-388`, `:589-603`, `:658-674`). The transfer applies to future rules where both inputs change.

Type shape:

```text
join_delta(left_delta, left_trace, right_delta, right_trace, time)
  -> output_delta
```

Sequence:

```text
new left batch  x accepted right trace
accepted left trace x new right batch
new/new cross term counted once by a fixed ownership rule
consolidate output before feedback
```

Current `dd_plan` already emits keyed arrangements, join operators, and delta wires on both inputs (`v6/prolog/compile/6_emit_dd_plan.pl:156-204`, `:324-340`, `:399-411`). A runtime convention can own the fresh/stable split. If plans must make cross-term ownership auditable, the join operator needs input roles or a scheduling field.

### 4. Early multiplicity consolidation

Estimated current reach benchmark win: unisolated. The current SQLite paths use primary keys and `INSERT OR IGNORE`, so they already deduplicate node frontiers (`v6/sprefa-store/src/engine.rs:350-461`, `:589-688`). DD preserves integer differences and cancels equal records before further work (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/consolidation.rs:18-36`, `~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/differential-dataflow-0.25.1/src/trace/implementations/merge_batcher.rs:277-339`). This transfer matters when several derivations produce opposing or repeated weights for the same tuple.

The existing signed-arrangement and delta-wire vocabulary can express the data semantics (`v6/prolog/compile/6_emit_dd_plan.pl:124-147`, `:372-420`). Batching boundaries and the timestamp at which cancellation is safe follow the choices in forks 1 and 2.

## Decisions

This recon records source findings and leaves the language/IR choice open.

1. The primary measured mechanism is the two-pass SQLite algorithm versus signed, timestamped fixed-point propagation.
2. Ordered immutable batches and fueled consolidation are a separate physical representation mechanism whose cost overlaps the algorithmic split.
3. Full-join repetition and SCC decomposition do not explain the measured reachability result.
4. The existing `dd_plan` vocabulary is sufficient only if timestamp, feedback, threshold, batching, compaction, and half-join ownership remain backend conventions. Auditable or selectable behavior requires explicit plan fields for the selected mechanisms.

Rejected explanation: file I/O or WAL is the primary cause. TEMP working tables were already memory-backed, and the matched in-memory database reduced runtime by 3.203% and 6.120% (`v6/sprefa-store/src/engine.rs:100-157`, `v6/sprefa-store/examples/perf_report.rs:95-126`).

Rejected explanation: the `sqlite-count-scc` label measures an SCC algorithm. Its call path delegates to the two-pass cone implementation (`v6/sprefa-store/examples/perf_report.rs:243-256`, `v6/sprefa-store/src/engine.rs:330-349`).

## Verification

Executed from `v6/sprefa-store`:

```text
cargo check --example perf_report
cargo build --release --example perf_report
target/release/examples/perf_report sqlite-count-scc 6 160000 0
target/release/examples/perf_report sqlite-dred-loop 6 160000 0
target/release/examples/perf_report dd 6 160000 0
DL_SQLITE_RAM_PROBE=1 target/release/examples/perf_report sqlite-count-scc 6 160000 0
DL_SQLITE_RAM_PROBE=1 target/release/examples/perf_report sqlite-dred-loop 6 160000 0
DL_SQLITE_RAM_PROBE=1 DL_CASCADE_TRACE=1 target/release/examples/perf_report sqlite-count-scc 6 160000 0
```

Both builds passed. They emitted the pre-existing `dead_code` warning for `Engine::name` and macOS linker minimum-version warnings during the release link. Every matched result had 800002 survivors, input hash `ef153ee39296ef0f`, and output hash `dd6a707234617ea5`.

Source-audit searches:

```text
cargo tree -i differential-dataflow
cargo tree -i timely
rg -n "scc_scope|scc_frontier|scc_next|scc_live" v6/sprefa-store/src
```

No todo markers were added, so `plans/PLANS.md` regeneration is unnecessary.

## Staffing

- Lane: `sol`, recon worktree.
- Base: `7e477da12d646f39e0137d9cc94dce93e7d76264`.
- Scope: one benchmark probe flag, registry and repository source audit, two recon documents, and the root closeout report.
- Dependency changes: none.
- Open design work: select whether nested time, threshold state, batch policy, and half-join ownership remain runtime conventions or become explicit `dd_plan` notions.
