# sqlite_ivm prepared-statement cache experiment

Date: 2026-09-09

Change: `Table::attach` calls `conn.set_prepared_statement_cache_capacity(128)` after constructing its non-owning rusqlite `Connection`. Rusqlite's default is 16.

Input database: `/private/tmp/dl7-query-review.ZTSjQZ/facts.db`, a temporary copy of the recovered extractor export. Original: `/private/tmp/extract-rusqlite-smoke.zYcTlY/generated.db`.

- 6,782 `node` rows
- 6,714 `edge` rows
- 5,771 `review_keyed_state` rows
- edited `node._row=2`: 112 outgoing CST edges, zero incoming CST edges
- query shape: manually keyed `JOIN ... ON` plus 31 retained top-level `WHERE` conjuncts; the planned emitter rewrite was not measured here

Native library hashes:

- cache 16 baseline: `dd834d231d3b5a7dfd4f59c3e77ec303d3c69db703038f549709a6829dbca9b4`
- cache 128 candidate: `2fa81af76b8c34a4c84e4f3d70fa9a2b1d8cc520c0cf5f7700c63136866bc523`

Warm paired six-update run, UPDATE statement wall time in milliseconds:

- cache 16: 409.464, 273.493, 253.085, 298.207, 293.045, 268.020; median 283.269
- cache 128: 62.976, 57.149, 61.677, 57.999, 57.373, 58.789; median 58.394
- median difference: -79.4%

Both runs ended with exact defining-SELECT parity: `missing=0`, `extra=0`.

Logs retained under `/private/tmp/dl7-query-review.ZTSjQZ/`:

- `resident-cache16-repeat.log`
- `resident-cache128.log`
- `resident-cache16-warm2.log`
- `resident-cache128-warm2.log`
- preserved original: `resident.log`

Focused checks:

- `2_vtab::defensive_shadow_protection_allows_source_dml_and_native_lifecycle`: passed
- `5_transactions::wal_snapshots_writer_contention_and_failed_maintenance_are_atomic`: passed
- native `scripts/4_lifecycle.sh` against the release extension: passed

Bound: up to 128 cached prepared statements per attached virtual-table `Table` wrapper. Prepared-statement memory is not byte-bounded here and multiplies with attached maintained views. This result is specific to the 31-map review query; it does not measure the emitter's revised ON-clause shape.

## Final emitted query

The final DL7 emitter generates explicit connected `INNER JOIN ... ON` clauses.
For multiple sources, base and joined-source constraints live in the first
applicable join condition. This avoids the IVM planner's separate Map per
top-level WHERE conjunct. Disconnected products are rejected; connected goals
are scheduled by already-bound variables.

The public `just sqlite-query ... --query` returned 5,771 rows exactly matching
the independent defining SELECT. `--install cli_cst` on a fresh temporary
extract database also returned those same 5,771 rows. The original export was
opened read-only; all mutations used temporary copies.

Final installed database: `/private/tmp/dl7-query-review.ZTSjQZ/cli-final.db`.
Logs: `cli-final-install.json`, `resident-final.sql`, and `resident-final.log`
in the same directory. The native library is the cache-128 build above.

One connection performed six committed updates of the same node, alternating
its kind and restoring the original value on the sixth update:

- First UPDATE after opening: 59.690 ms.
- Next five UPDATEs: 15.910, 15.468, 15.633, 15.868, 15.124 ms.
- Warm UPDATE median: 15.633 ms.
- Corresponding five COMMITs: 3.170, 4.382, 3.557, 4.461, 3.069 ms.
- Final defining-SELECT comparison: missing 0, extra 0.

These are local smoke measurements for one root-node update with 112 outgoing
edges. They do not measure whole-file replacement, cross-repository edit
batches, effect delivery, or the cold installation time of the final SQL shape.

## Verification

- `just --justfile v7/justfile sqlite-query-test`: 5/5 passed.
- `just --justfile v7/justfile interned-storage-test`: 1/1 passed.
- Native lifecycle and transaction checks listed above passed.
- Coverage was added to local executable checks. CI wiring was not changed.

The layout test verifies actual compile-time DL7 derivation and extracts the
closed artifact rows. Full second-evaluation layout emission remains a
separate compiler integration limitation, documented in `v7/README.md`.
