# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

**Ran with DL6_BENCH_UNBATCH=1**, so per-statement cost is attributed and the total reads high against a real tick.

Node v24.15.0 · darwin/arm64 · 2026-08-06T18:56:47.387Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 26 | 2206 | 484,678 | 732 MB |

## `grid_10000`: where the fixpoint went

17 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 1395.7 | 63.5% | 1 | 0 | `INSERT INTO "__support_next_reachable" ("source", "target", "__refcount") WITH RECURSIVE "reachable" ("source", "target") AS (SELECT b0."source" AS "s` |
| 298.8 | 13.6% | 1 | 0 | `INSERT INTO "__delta_reachable" ("_sign", "_sequence", "source", "target") SELECT 1, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 200.6 | 9.1% | 1 | 0 | `INSERT INTO "__frontier_reachable" ("_phase", "_sequence", "source", "target") SELECT ?, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 139.1 | 6.3% | 1 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n` |
| 138.8 | 6.3% | 1 | 0 | `INSERT INTO "__new_reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n LEFT ` |
| 8.1 | 0.4% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 7.9 | 0.4% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 3.4 | 0.2% | 1 | 0 | `INSERT INTO "__delta_edge" ("_sign", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(value, '` |
| 3.0 | 0.1% | 1 | 0 | `INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(valu` |
| 1.0 | 0.0% | 1 | 0 | `INSERT INTO "__delta_reachable" ("_sign", "_sequence", "source", "target") SELECT -1, row_number() OVER () - 1, "source", "target" FROM "reachable" WH` |
| 0.2 | 0.0% | 1 | 1 | `SELECT CASE WHEN EXISTS (SELECT 1 FROM "__delta_edge" WHERE "_sign" = -1 LIMIT 1) OR EXISTS (SELECT 1 FROM "__delta_reachable" WHERE "_sign" = -1 LIMI` |
| 0.2 | 0.0% | 1 | 0 | `UPDATE "reachable" AS h SET "__refcount" = COALESCE((SELECT n."__refcount" FROM "__support_next_reachable" n WHERE n."source" = h."source" AND n."targ` |
