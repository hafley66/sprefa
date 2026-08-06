# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

Node v24.15.0 · darwin/arm64 · 2026-08-06T20:51:11.113Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 17 | 1998 | 535,135 | 731 MB |
| `layered_10000` | 9,984 | 9,951,396 | `addcf85b5162b9da` | 7 | 19506 | 510,171 | 2602 MB |
| `chain_10000` | 7,743 | 9,996,213 | `df09b2f409f8b9a8` | 5 | 30670 | 325,928 | 3997 MB |

## `grid_10000`: where the fixpoint went

96 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 794.8 | 40.0% | 1 | 0 | `BATCH x8: UPDATE "reachable" \| INSERT "__delta_reachable" \| DELETE "reachable" \| DELETE "__new_reachable" \| INSERT "__new_reachable" \| INSERT "__delta` |
| 586.5 | 29.5% | 44 | 0 | `BATCH x3: DELETE "__expand_a_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 582.3 | 29.3% | 44 | 0 | `BATCH x3: DELETE "__expand_b_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 9.1 | 0.5% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 7.8 | 0.4% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 7.0 | 0.4% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 1.1 | 0.1% | 1 | 0 | `BATCH x5: DELETE "__support_next_reachable" \| DELETE "__expand_a_reachable" \| DELETE "__expand_b_reachable" \| INSERT "__expand_a_reachable" \| INSERT "` |
| 0.2 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) OR EXISTS (SELECT 1 FROM "__delta_reachable" WHERE "_sign" = -1 LIMI` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"; DELETE FROM "__delta_reachable"; DELETE FROM "__next_frontier_reachable"` |
| 0.1 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) OR EXISTS (SELECT 1 FROM "__next_frontier_reachable" LIMIT 1) THEN 1 ELSE 0 END` |

## `layered_10000`: where the fixpoint went

200 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 7543.5 | 38.7% | 1 | 0 | `BATCH x8: UPDATE "reachable" \| INSERT "__delta_reachable" \| DELETE "reachable" \| DELETE "__new_reachable" \| INSERT "__new_reachable" \| INSERT "__delta` |
| 5918.7 | 30.4% | 96 | 0 | `BATCH x3: DELETE "__expand_b_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 5916.3 | 30.3% | 96 | 0 | `BATCH x3: DELETE "__expand_a_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 79.5 | 0.4% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 16.9 | 0.1% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 16.8 | 0.1% | 1 | 9,984 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 2.5 | 0.0% | 1 | 0 | `BATCH x5: DELETE "__support_next_reachable" \| DELETE "__expand_a_reachable" \| DELETE "__expand_b_reachable" \| INSERT "__expand_a_reachable" \| INSERT "` |
| 0.2 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) OR EXISTS (SELECT 1 FROM "__delta_reachable" WHERE "_sign" = -1 LIMI` |
| 0.0 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"; DELETE FROM "__delta_reachable"; DELETE FROM "__next_frontier_reachable"` |
| 0.0 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) OR EXISTS (SELECT 1 FROM "__next_frontier_reachable" LIMIT 1) THEN 1 ELSE 0 END` |

## `chain_10000`: where the fixpoint went

2589 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 11498.3 | 37.5% | 1290 | 0 | `BATCH x3: DELETE "__expand_a_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 11457.6 | 37.4% | 1291 | 0 | `BATCH x3: DELETE "__expand_b_reachable" \| WITH "reachable" \| INSERT "__support_next_reachable"` |
| 7590.3 | 24.8% | 1 | 0 | `BATCH x8: UPDATE "reachable" \| INSERT "__delta_reachable" \| DELETE "reachable" \| DELETE "__new_reachable" \| INSERT "__new_reachable" \| INSERT "__delta` |
| 77.5 | 0.3% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 13.4 | 0.0% | 1 | 0 | `BATCH x2: INSERT "__delta_edge" \| INSERT "__frontier_edge"` |
| 13.0 | 0.0% | 1 | 7,743 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 1.8 | 0.0% | 1 | 0 | `BATCH x5: DELETE "__support_next_reachable" \| DELETE "__expand_a_reachable" \| DELETE "__expand_b_reachable" \| INSERT "__expand_a_reachable" \| INSERT "` |
| 0.3 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) OR EXISTS (SELECT 1 FROM "__delta_reachable" WHERE "_sign" = -1 LIMI` |
| 0.1 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__next_frontier_edge" LIMIT 1) OR EXISTS (SELECT 1 FROM "__next_frontier_reachable" LIMIT 1) THEN 1 ELSE 0 END` |
| 0.0 | 0.0% | 1 | 0 | `DELETE FROM "__delta_edge"; DELETE FROM "__next_frontier_edge"; DELETE FROM "__delta_reachable"; DELETE FROM "__next_frontier_reachable"` |
