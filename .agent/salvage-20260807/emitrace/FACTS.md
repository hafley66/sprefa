# dl6 core benchmark facts

Regenerate: `just dl6-bench` from `v6/`. One run per case, no best-of, so
treat single-digit percent moves as noise.

| env | effect |
|---|---|
| `DL6_BENCH_FULL=1` | adds `layered_10000` and `chain_10000`, about 35s more |
| `DL6_BENCH_UNBATCH=1` | runs batched statements one at a time so cost lands per statement; drops atomicity, so totals read high |
| `DL6_BENCH_WORK=<dir>` | where generated inputs live, default `dl6/.bench` |

**Ran with DL6_BENCH_UNBATCH=1**, so per-statement cost is attributed and the total reads high against a real tick.

Node v24.15.0 · darwin/arm64 · 2026-08-06T22:05:11.112Z

## Contract numbers

| case | edges | derived | checksum | load ms | fixpoint ms | rows/sec | peak RSS |
|---|---|---|---|---|---|---|---|
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 20 | 2074 | 515,526 | 713 MB |
| `layered_10000` | 9,984 | 9,951,396 | `addcf85b5162b9da` | 6 | 20077 | 495,662 | 1796 MB |
| `chain_10000` | 7,743 | 9,996,213 | `df09b2f409f8b9a8` | 6 | 34213 | 292,176 | 1796 MB |

## `grid_10000`: where the fixpoint went

284 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 389.0 | 18.9% | 44 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_b_reachable") INSERT OR IGNORE INTO "__expand_a_reachable" ("source` |
| 387.2 | 18.8% | 44 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_a_reachable") INSERT OR IGNORE INTO "__expand_b_reachable" ("source` |
| 310.1 | 15.0% | 1 | 0 | `INSERT INTO "__delta_reachable" ("_sign", "_sequence", "source", "target") SELECT 1, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 228.4 | 11.1% | 45 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_a_reachable"` |
| 225.3 | 10.9% | 44 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_b_reachable"` |
| 207.7 | 10.1% | 1 | 0 | `INSERT INTO "__frontier_reachable" ("_phase", "_sequence", "source", "target") SELECT ?, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 143.9 | 7.0% | 1 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n` |
| 141.1 | 6.8% | 1 | 0 | `INSERT INTO "__new_reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n LEFT ` |
| 9.2 | 0.4% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 7.6 | 0.4% | 1 | 3,960 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
| 3.4 | 0.2% | 1 | 0 | `INSERT INTO "__delta_edge" ("_sign", "_sequence", "source", "target") SELECT json_extract(value, '?'), json_extract(value, '?'), json_extract(value, '` |
| 3.1 | 0.1% | 45 | 0 | `DELETE FROM "__expand_a_reachable"` |

## `layered_10000`: where the fixpoint went

596 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 3852.8 | 19.2% | 96 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_a_reachable") INSERT OR IGNORE INTO "__expand_b_reachable" ("source` |
| 3845.2 | 19.2% | 96 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_b_reachable") INSERT OR IGNORE INTO "__expand_a_reachable" ("source` |
| 2979.9 | 14.9% | 1 | 0 | `INSERT INTO "__delta_reachable" ("_sign", "_sequence", "source", "target") SELECT 1, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 2231.7 | 11.1% | 96 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_b_reachable"` |
| 2222.8 | 11.1% | 97 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_a_reachable"` |
| 1971.1 | 9.8% | 1 | 0 | `INSERT INTO "__frontier_reachable" ("_phase", "_sequence", "source", "target") SELECT ?, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 1437.9 | 7.2% | 1 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n` |
| 1363.4 | 6.8% | 1 | 0 | `INSERT INTO "__new_reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n LEFT ` |
| 81.4 | 0.4% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 19.3 | 0.1% | 97 | 0 | `DELETE FROM "__expand_a_reachable"` |
| 18.9 | 0.1% | 97 | 0 | `DELETE FROM "__expand_b_reachable"` |
| 17.5 | 0.1% | 1 | 9,984 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |

## `chain_10000`: where the fixpoint went

7763 statements ran during the tick.

| ms | share | calls | rows out | SQL shape |
|---|---|---|---|---|
| 9148.5 | 26.8% | 1291 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_b_reachable"` |
| 9102.8 | 26.6% | 1291 | 0 | `INSERT OR IGNORE INTO "__support_next_reachable" ("source", "target", "__refcount") SELECT "source", "target", 1 FROM "__expand_a_reachable"` |
| 3954.6 | 11.6% | 1291 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_a_reachable") INSERT OR IGNORE INTO "__expand_b_reachable" ("source` |
| 3913.2 | 11.5% | 1290 | 0 | `WITH "reachable" ("source", "target") AS (SELECT "source", "target" FROM "__expand_b_reachable") INSERT OR IGNORE INTO "__expand_a_reachable" ("source` |
| 3027.8 | 8.9% | 1 | 0 | `INSERT INTO "__delta_reachable" ("_sign", "_sequence", "source", "target") SELECT 1, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 1976.0 | 5.8% | 1 | 0 | `INSERT INTO "__frontier_reachable" ("_phase", "_sequence", "source", "target") SELECT ?, "rowid" - 1, "source", "target" FROM "__new_reachable"` |
| 1409.6 | 4.1% | 1 | 0 | `INSERT OR IGNORE INTO "reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n` |
| 1372.2 | 4.0% | 1 | 0 | `INSERT INTO "__new_reachable" ("source", "target", "__refcount") SELECT n."source", n."target", n."__refcount" FROM "__support_next_reachable" n LEFT ` |
| 86.2 | 0.3% | 1 | 0 | `DELETE FROM "__frontier_edge"; INSERT INTO "__frontier_edge" ("_phase", "_sequence", "source", "target") SELECT "_phase", "_sequence", "source", "targ` |
| 70.4 | 0.2% | 1292 | 0 | `DELETE FROM "__expand_b_reachable"` |
| 69.0 | 0.2% | 1291 | 0 | `DELETE FROM "__expand_a_reachable"` |
| 13.5 | 0.0% | 1 | 7,743 | `INSERT OR IGNORE INTO "edge" ("source", "target") SELECT json_extract(value, '?'), json_extract(value, '?') FROM json_each(?) RETURNING "source", "tar` |
