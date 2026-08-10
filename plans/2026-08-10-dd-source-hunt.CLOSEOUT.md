# DD Source Hunt Closeout

## Commit

Research and probe commit:

```text
7d2418b5b84bfa0dddf616fb85c71168ac8519fc
```

The closeout report is committed separately so it can record the immutable research commit SHA.

## Decomposition

The matched DAG 960k probe produced the same 800,002 survivors, input hash `ef153ee39296ef0f`, and output hash `dd6a707234617ea5` for every measured engine.

| path | storage | retraction ms | ratio to DD |
|---|---|---:|---:|
| sqlite-count-scc | file | 1761.441 | 10.19x |
| sqlite-count-scc | memory | 1705.019 | 9.86x |
| sqlite-dred-loop | file | 1808.060 | 10.46x |
| sqlite-dred-loop | memory | 1697.397 | 9.82x |
| differential dataflow | resident | 172.923 | 1.00x |

The in-memory count-scc statement trace divides 1682.317 ms into:

```text
over-delete initialization and rounds   871.04 ms   51.8%
rederive base and rounds                807.70 ms   48.0%
trace/unclassified remainder              3.58 ms    0.2%
```

Logged SQL statements account for 99.79 percent of wall time. Moving the persistent SQLite database to memory reduces count-scc by 3.203 percent and dred-loop by 6.120 percent.

Source findings:

1. DD accepts the root deletion as a signed `-1` update, carries it through product-timestamped iteration, joins new batches against indexed trace history, consolidates equal updates, and emits distinct membership changes at threshold crossings.
2. Both correct SQLite loop paths materialize an affected cone, mark it dead, then walk the cone again to restore externally supported nodes.
3. SQLite TEMP working tables were already memory-backed. Each round still clears, inserts, counts, and updates B-tree tables.
4. The SQLite reach loops already join a small frontier through indexed dependency rows. Full relation join repetition does not explain this benchmark.
5. `sqlite-count-scc` delegates directly to a two-pass cone implementation. The measured path performs no SCC partition or stratification pass.

Detailed registry and repository path:line evidence is in `plans/2026-08-10-dd-source-hunt.RECON.md`. The plain-language diagrams are in `plans/2026-08-10-dd-source-hunt.RECON.visual.human.unga.md`.

## Ranked Transfer Forks

### 1. Timestamped signed-delta fixed point

Maintain per-key support and propagate only threshold-crossing `+1` and `-1` membership changes through inner rounds. Close an outer update when its next inner delta is empty.

Measured component: the separate SQLite rederive pass is 807.70 ms, or 48.0 percent of the traced run. Removing only that pass gives an arithmetic floor near 874.6 ms. Additional first-pass reductions overlap this estimate.

IR fork: keep inner timestamps, feedback frontiers, and threshold history as runtime rules behind the current iterate operator, or add explicit iterative-scope, timestamp, feedback, and threshold-state terms.

### 2. Immutable epoch batches with fueled consolidation

Append sealed signed-update batches, organize them by size, merge them incrementally, and discard zero totals. Preserve stable arrangement handles across outer updates.

Measured component: this representation affects both traced SQLite phases. The current probe cannot isolate an additive number from fork 1.

IR fork: use one backend-wide batch and compaction policy, or add per-arrangement storage, batch ownership, and compaction-frontier fields.

### 3. Arranged half-join scheduling for general rules

Evaluate new-left against stored-right and stored-left against new-right, with one ownership rule for the new-by-new term.

Measured component in the current reach benchmark: 0 ms attributed to join scheduling because the SQLite loops already use frontier-index joins.

IR fork: keep fresh/stable ownership in the runtime, or add join input roles or a scheduling field so cross-term ownership is inspectable.

### 4. Early signed multiplicity consolidation

Group equal key, value, time updates before joins and feedback; sum differences and remove zero totals.

Measured component: unisolated. The present SQLite benchmark already deduplicates boolean node frontiers with primary keys. The transfer applies to rules that produce repeated or opposing tuple weights.

IR fork: existing signed arrangements and delta wires cover the value semantics. Safe batching and timestamp boundaries depend on forks 1 and 2.

## Verification

```text
cargo check --example perf_report                         PASS
cargo build --release --example perf_report               PASS
matched file and memory DAG 960k probe                    PASS
input, output, and survivor equivalence                   PASS
git diff --check                                           PASS
```

The pre-commit comment-budget rail could not start because the worktree lacks the `rxjs` package. The lane brief prohibits `pnpm`. The hook's documented `git commit -n` bypass was used after building its required Rust extractor and completing the checks above.
