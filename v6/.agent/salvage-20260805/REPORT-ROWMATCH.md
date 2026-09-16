# REPORT — hoist the per-call CREATE TEMP in loadRowMatchCandidates

Branch: `lab/rowmatch`. Base `4e57c322`, `git merge --ff-only 4e57c322` clean.

## Diff summary

- `v6/dl/src/3_runtime.ts`
  - Added module scope `const rowMatchTableReady = new WeakSet<Db>();` next to the
    `ROW_MATCH_TEMP_TABLE` / `ROW_MATCH_INSERT_CHUNK` constants.
  - `loadRowMatchCandidates`: inside the `defer`, mark the db and emit the
    `CREATE TEMP TABLE IF NOT EXISTS _row_match_candidates (...)` statement only on the
    first call for that connection; `DELETE FROM` stays per call; INSERT chunking
    untouched. Set membership records "CREATE was issued", which `IF NOT EXISTS` makes
    idempotent. Width check: `relMaxColumnWidth(relDecls)` is fixed for a runtime's
    lifetime, no re-create logic added.
- `v6/dl/tests/3_runtime.test.ts`
  - Added one COUNT test: two separate EDB insert commits on the same Db, both through
    `loadRowMatchCandidates` (each insert commit hits `preCheckExistingKeys` on a
    non-empty insert set once), SQL captured over `PERF_CHANNEL_NAMES.sql`; asserts
    exactly one `CREATE TEMP TABLE IF NOT EXISTS _row_match_candidates` and exactly two
    `DELETE FROM _row_match_candidates`.

## Validation

`cd v6/dl && pnpm test` (node_modules + `v6/sprefa-store/js` installed first, rxjs was
missing):

```
✔ COUNT: loadRowMatchCandidates emits its CREATE TEMP once per Db, and a DELETE per call, across two commits (106.5515ms)
...
ℹ pass 98
ℹ fail 0
```

## Statement-count receipts (4_ingest.test.ts, `DL_PERF_LOG=$PWD/perf.jsonl`)

Before / after for `CREATE TEMP TABLE IF NOT EXISTS _row_match_candidates`:

- before: 180 CREATE per run
- after: 8 (one per connection in the run; DELETE stays 180 per call, untouched)

```
$ node ../tools/perf-n1.mjs perf.jsonl | grep -i "create temp"
-	CREATE TEMP TABLE IF NOT EXISTS _row_match_candidates (c?, c?, c?, c?, c?, c?, c?)	8	0	0.6200000000000001	0
```

## Top-6 table (by duration, `sort -t$'\t' -k3,3nr`)

```
-	DELETE FROM _row_match_candidates	180	0	4.239999999999994	0
-	BEGIN IMMEDIATE	74	0	1.990000000000001	0
-	COMMIT	74	0	8.159999999999986	0
-	SELECT CASE WHEN path = -? THEN NULL ELSE (SELECT content FROM strings WHERE string_id = path) END AS path, CASE WHEN fa	74	0	12.519999999999998	0
-	SELECT CASE WHEN path = -? THEN NULL ELSE (SELECT content FROM strings WHERE string_id = path) END AS path, owner_start,	74	0	3.290000000000002	0
-	INSERT INTO _row_match_candidates(c?, c?, c?, c?, c?, c?) VALUES (?,?,?,?,?,-?),(?,?,?,?,?,-?),(?,?,?,?,?,-?),(?,?,?,?,?	42	0	2.9599999999999973	0
```
