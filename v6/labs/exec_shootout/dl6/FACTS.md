# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

**Ran with DL6_BENCH_UNBATCH=1**, so per-statement cost is attributed and the total reads high against a real tick.

Node v24.15.0 · darwin/arm64 · 2026-08-07T21:37:28.047Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 22 | 1182 | 904,569 | 621 MB |
| `layered_10000` | 9,984 | 9,951,396 | `addcf85b5162b9da` | 5 | 11680 | 852,003 | 1254 MB |
| `chain_10000` | 7,743 | 9,996,213 | `df09b2f409f8b9a8` | 4 | 21196 | 471,608 | 1447 MB |

## `grid_10000`: where the fixpoint went

278 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 365.9 | 31.2% | 45 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 365.1 | 31.1% | 44 | 0 | `INSERT OR IGNORE INTO "__pong_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 211.2 | 18.0% | 45 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__ping_reachable"` |
| 210.7 | 18.0% | 44 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__pong_reachable"` |
| 7.6 | 0.7% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 3.4 | 0.3% | 1 | 0 | `INSERT INTO "__delta_edge" ("_sign", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(value, '` |
| 3.1 | 0.3% | 1 | 0 | `INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(valu` |
| 2.5 | 0.2% | 45 | 0 | `DELETE FROM "__pong_reachable"` |
| 2.4 | 0.2% | 45 | 0 | `DELETE FROM "__ping_reachable"` |
| 1.3 | 0.1% | 1 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b0."target" AS "target" ` |
| 0.2 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |

## `layered_10000`: where the fixpoint went

590 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 3683.2 | 31.6% | 97 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 3678.2 | 31.5% | 96 | 0 | `INSERT OR IGNORE INTO "__pong_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 2117.5 | 18.2% | 97 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__ping_reachable"` |
| 2112.8 | 18.1% | 96 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__pong_reachable"` |
| 19.3 | 0.2% | 97 | 0 | `DELETE FROM "__pong_reachable"` |
| 18.7 | 0.2% | 97 | 0 | `DELETE FROM "__ping_reachable"` |
| 16.7 | 0.1% | 1 | 9,984 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 8.7 | 0.1% | 1 | 0 | `INSERT INTO "__delta_edge" ("_sign", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(value, '` |
| 7.6 | 0.1% | 1 | 0 | `INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(valu` |
| 3.4 | 0.0% | 1 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b0."target" AS "target" ` |
| 0.1 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |

## `chain_10000`: where the fixpoint went

7757 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 7278.6 | 34.4% | 1291 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__ping_reachable"` |
| 7261.3 | 34.3% | 1291 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target") SELECT "source", "target" FROM "__pong_reachable"` |
| 3258.7 | 15.4% | 1291 | 0 | `INSERT OR IGNORE INTO "__pong_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 3238.1 | 15.3% | 1291 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b1."target" AS "target" ` |
| 49.7 | 0.2% | 1292 | 0 | `DELETE FROM "__pong_reachable"` |
| 49.4 | 0.2% | 1291 | 0 | `DELETE FROM "__ping_reachable"` |
| 12.9 | 0.1% | 1 | 7,743 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 6.7 | 0.0% | 1 | 0 | `INSERT INTO "__delta_edge" ("_sign", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(value, '` |
| 6.1 | 0.0% | 1 | 0 | `INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(valu` |
| 2.5 | 0.0% | 1 | 0 | `INSERT OR IGNORE INTO "__ping_reachable" ("source", "target") SELECT "source", "target" FROM (SELECT b0."source" AS "source", b0."target" AS "target" ` |
| 0.2 | 0.0% | 2 | 2 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) THEN 1 ELSE 0 END AS has_retraction` |
| 0.1 | 0.0% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
