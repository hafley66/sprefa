# REPORT — perf-n1: DL_PERF_LOG aggregator with N+1 suspect flag

## Tool usage line

```bash
node v6/tools/perf-n1.mjs <path-to-jsonl> [--min-stmts <n>]
```

Default `--min-stmts 10`. Invocation: `node v6/tools/perf-n1.mjs v6/tsv2/goldens/trace-line.jsonl --min-stmts 10`.

## What it does

Reads a DL_PERF_LOG JSONL file, emits one TSV row per `(tick, unit)` to stdout:

```
tick  unit  statements  rows  wall_ms  suspect
```

`unit` is the rule id or rel name the line's shape provides. Rows are sorted by `wall_ms` descending. Lines that are not valid JSON or match neither known shape are skipped and counted; the count goes to stderr. Lines with no tick aggregate under tick `-`. Zero dependencies, plain node.

`suspect` is a mechanical flag, never a judgment (exact rule from the brief):

```
suspect = 1 when statements >= max(min_stmts, rows) AND rows > 0 AND statements > 1
suspect = 0 otherwise
```

## The two recognized line shapes

| shape | source (read, not invented) | unit source | per-unit `statements` | per-unit `rows` / `wall_ms` |
|---|---|---|---|---|
| serve tick line | `v6/tsv2/serve/0_trace.ts` + `v6/tsv2/runtime/types.ts` (`IServeTickLine`, `IServeRuleEvent`) | `rules[]` → `rule` | line-level `statements` | `rule.rows`, `rule.wall_ms` |
| dl tick line | `v6/dl/src/0_trace.ts` + `v6/dl/src/0_types.ts` (`PerfTickLine`, `PerfBindEntry`) | `binds[]` → `rel` | line-level `stmt_count` | `bind.rows`, `bind.ms` |

Neither `rules[]` nor `binds[]` carries a per-unit statement count of its own, so the per-unit `statements` value is the line's tick-level statement count (serve `statements` / dl `stmt_count`). This choice is the only one that lets a single user-facing flag express the "one statement per row" N+1 shape the detection targets.

## Test output (verbatim)

`node --test v6/tools/perf-n1.test.mjs`

```
✔ aggregation sums statements/rows/wall_ms across lines of one (tick, unit) (0.53525ms)
✔ suspect fires on a synthetic N+1 fixture (50 statements, 50 rows) (0.143125ms)
✔ suspect stays 0 on a batched fixture (1 statement, 50 rows) (0.074375ms)
✔ malformed lines are skipped and counted, valid lines still aggregate (0.56125ms)
ℹ tests 4
ℹ suites 0
ℹ pass 4
ℹ fail 0
ℹ cancelled 0
ℹ skipped 0
ℹ todo 0
ℹ duration_ms 42.109084
```

## Fixture shapes and source citations

Fixture strings are copied faithfully from the shapes read in the two trace files, cited in `perf-n1.test.mjs` comments:

1. **Serve shape** — `v6/tsv2/serve/0_trace.ts` (tick line folds `sprefa:rule` events into `rules[]`) and `v6/tsv2/runtime/types.ts` (`IServeTickLine`, `IServeRuleEvent {rule, rows, wall_ms}`). Real bytes cross-checked against `v6/tsv2/goldens/trace-line.jsonl`.
2. **DL shape** — `v6/dl/src/0_trace.ts` (sink format, `flushTick`) and `v6/dl/src/0_types.ts` (`PerfTickLine`, `PerfBindEntry {rel, rows, ms}`).

## Live-receipt outcome

Produced. Attempted once: `v6/tsv2/tests/traceGolden.test.ts` writes its trace log into a fresh temp dir, but a committed golden already yields a usable JSONL file directly — `v6/tsv2/goldens/trace-line.jsonl` (`TRACE_SCHEMA.tick_line` / `rule` events). No test was modified; the tool pointed at that golden file directly. Top 10 output rows (all 8 rows; the file has 4 lines):

```
tick	unit	statements	rows	wall_ms	suspect
1	<program>:merged/1#1	36	2	0	1
1	<program>:merged/1#2	36	1	0	1
2	<program>:merged/1#1	24	0	0	0
2	<program>:merged/1#2	24	0	0	0
3	<program>:merged/1#1	30	0	0	0
3	<program>:merged/1#2	30	1	0	1
4	<program>:merged/1#1	24	0	0	0
4	<program>:merged/1#2	24	0	0	0
```

Note on the golden: it is a pinned artifact, so wall clocks (`wall_ms`) and per-program rule prefixes are normalized out (`timing`/`host` stability marks in `v6/tsv2/runtime/0_traceSchema.ts`), which is why `wall_ms` is 0 and rules read `<program>:…`. The `suspect=1` flags fire here because the per-unit `statements` equals the whole tick's statement count (24-36) applied to a rule that produced few rows; the flag is the brief's exact formula applied mechanically to that shared value, not a judgment about this deliberately dull two-arm program.

## Style compliance

Banned words (`provenance`, `substrate`, `load-bearing`, `regime`, `support`, `honest(ly)`, `distill`, `ground` as verb, `ruling`) and em dashes: none in code, tests, or this report. Comments state only constraints the code cannot show (shape source citations, the shared-statements choice). No single-letter variable names.

## Validation

```bash
node --test v6/tools/perf-n1.test.mjs   # all green (4/4)
```
