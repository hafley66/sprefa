# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

Node v24.15.0 · darwin/arm64 · 2026-08-06T18:41:32.246Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 23 | 2200 | 486,000 | 724 MB |

## `grid_10000`: where the fixpoint went

7 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 2168.9 | 98.9% | 1 | 0 | `BATCH x10: DELETE "__support_next_reachable" \| INSERT "__support_next_reachable" \| UPDATE "reachable" \| INSERT "__delta_reachable" \| DELETE "reachable` |
| 8.0 | 0.4% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 7.8 | 0.4% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 6.9 | 0.3% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 0.2 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) OR EXISTS (SELECT 1 FROM "__delta_reachable" WHERE "_sign" = -1 LIMI` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"; DELETE FROM "__delta_reachable"; DELETE FROM "__next_frontier_reachable"` |
| 0.1 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) OR EXISTS (SELECT 1 FROM "__next_frontier_reachable" LIMIT 1) THEN 1 ELSE 0 END` |
