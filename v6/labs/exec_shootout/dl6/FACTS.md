# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

Node v24.15.0 · darwin/arm64 · 2026-08-10T18:09:58.419Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 21 | 1265 | 845,217 | 715 MB |
| `layered_10000` | 9,984 | 9,951,396 | `addcf85b5162b9da` | 5 | 11721 | 849,023 | 1266 MB |
| `chain_10000` | 7,743 | 9,996,213 | `df09b2f409f8b9a8` | 5 | 20850 | 479,435 | 1467 MB |

## `grid_10000`: where the fixpoint went

96 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 636.3 | 50.7% | 44 | 0 | `BATCH x3: DELETE "__pong_reachable" \| INSERT "__pong_reachable" \| INSERT "reachable"` |
| 601.4 | 47.9% | 44 | 0 | `BATCH x3: DELETE "__ping_reachable" \| INSERT "__ping_reachable" \| INSERT "reachable"` |
| 7.7 | 0.6% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 7.3 | 0.6% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 2.8 | 0.2% | 1 | 0 | `BATCH x6: DELETE "__new_reachable" \| DELETE "__ping_reachable" \| DELETE "__pong_reachable" \| INSERT "__ping_reachable" \| INSERT "__ping_reachable" \| I` |
| 0.2 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"` |
| 0.1 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) THEN 1 ELSE 0 END AS carry_pending` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |

## `layered_10000`: where the fixpoint went

200 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 5842.5 | 49.9% | 96 | 0 | `BATCH x3: DELETE "__ping_reachable" \| INSERT "__ping_reachable" \| INSERT "reachable"` |
| 5825.1 | 49.8% | 96 | 0 | `BATCH x3: DELETE "__pong_reachable" \| INSERT "__pong_reachable" \| INSERT "reachable"` |
| 17.0 | 0.1% | 1 | 9,984 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 17.0 | 0.1% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 6.4 | 0.1% | 1 | 0 | `BATCH x6: DELETE "__new_reachable" \| DELETE "__ping_reachable" \| DELETE "__pong_reachable" \| INSERT "__ping_reachable" \| INSERT "__ping_reachable" \| I` |
| 0.2 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 0.0 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"` |
| 0.0 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) THEN 1 ELSE 0 END AS carry_pending` |

## `chain_10000`: where the fixpoint went

2589 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 10402.7 | 49.9% | 1291 | 0 | `BATCH x3: DELETE "__pong_reachable" \| INSERT "__pong_reachable" \| INSERT "reachable"` |
| 10399.9 | 49.9% | 1290 | 0 | `BATCH x3: DELETE "__ping_reachable" \| INSERT "__ping_reachable" \| INSERT "reachable"` |
| 12.9 | 0.1% | 1 | 7,743 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 12.6 | 0.1% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 4.6 | 0.0% | 1 | 0 | `BATCH x6: DELETE "__new_reachable" \| DELETE "__ping_reachable" \| DELETE "__pong_reachable" \| INSERT "__ping_reachable" \| INSERT "__ping_reachable" \| I` |
| 0.2 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"` |
| 0.0 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) THEN 1 ELSE 0 END AS carry_pending` |
