# SQLite Native Take 2

Current gate: PASS, exit 0. 13 boundary tests, 6 query-family tests and the shared
171-state fixture in two logging modes on SQLite 3.53.2.
Final executable receipt: `42_sqlite_native_take2/receipts/m3-receipt.json`.
The full sweep and semantic receipts are retained alongside it. The receipt
reducer also checks every stderr record has only callback/depth fields and every
case stays within the 256-event cap.
Milestone 1 boundary gate at commit: PASS, 10 tests.
Upstream pinned SQLite FTS5: 3 savepoint tests and 27 conflict tests, zero errors.
Initial boundary attempt: exit 1, 9 passing tests and one incorrect expected
callback timeline. Focused repair: exit 0, 2 tests. Green gate: exit 0.
Logs retained under `42_sqlite_native_take2/receipts/`; source/build/log SHA256 and
exact commands in `receipts/receipt.json`. No previous passing lifecycle result
was inferred from Terra's failed testfixture build.

Worktree base: `46e918dac`; task brief commit: `0aa441195`.
Milestone 1 commit: `176b45818`.
Milestone 2 commit: `bef056ccb`.
Milestone 3 commit: `3d764fe1a`.
All commits are local; no pushes or merges. Gate runs stayed within the four-per-
milestone limit. Parent notifications were sent through boop without waiting.

## Commands

```sh
/opt/homebrew/bin/python3 v6/labs/exec_shootout/postgres_pglite_ivm/42_sqlite_native_take2/4_gate.py
# Task-owned upstream build, using an archive of the pinned SQLite source:
/tmp/sprefa-sqlite-native-take2/upstream/configure --enable-fts5 --with-tcl=/opt/homebrew/opt/tcl-tk@8/lib
make -j2 testfixture sqlite3
./testfixture /tmp/sprefa-sqlite-native-take2/upstream/ext/fts5/test/fts5savepoint.test
./testfixture /tmp/sprefa-sqlite-native-take2/upstream/ext/fts5/test/fts5conflict.test
```

The configure, make and testfixture commands run in
`/tmp/sprefa-sqlite-native-take2/build`. Existing Tcl is used without installation.

CI coverage added: standalone C extension compilation, 13 boundary tests,
6 query-family tests and shared fixture execution. Boundary additions reject
illegal shadow-trigger virtual-table reentry, check deferred foreign-key COMMIT
failure and outermost-savepoint release durability.
Existing build/test coverage is unchanged. No repository-wide CI was run or edited.

Boundary covers autocommit multi-row writes, explicit-transaction read visibility,
nested savepoints/release/rollback-to/outer rollback, source ABORT/FAIL/IGNORE/
REPLACE/UPSERT/CHECK behavior, injected xSync rollback, reopen, module-absent
writer rejection, second loaded writer and WAL reader isolation, shadow OLD
mismatch, recursive-trigger enforcement, a separately labeled direct-event probe,
and bounded diagnostics. SQL counters roll back with SQLite shadow state.

Milestone 2: filter/projection bag supports, grouped COUNT/SUM, two-source join,
two-occurrence self join and three-source join pass fresh SQLite oracles through
duplicates, NULLs, multi-row changes, conflicts, reopen, savepoint churn and xSync
failure. A three-source accumulator-overflow case checks whole-statement rollback.
Six constructor-error red cases preceded implementation; their log is retained.
Green milestone gate receipt: `receipts/m2-receipt.json`.

Milestone 3: additive shared runner arms `sqlite-native-take2` and
`sqlite-native-take2-logged`, with a `take2` shell entry that uses the same oracle,
memory-pressure checks and deadlines without requiring PostgreSQL startup.
The bounded sweep passed 9 cells x 2 repetitions x 2 modes = 36 case runs,
180 mutation-state checks. Separate semantic run passed 171 states per mode.
Exact input/output hashes match; fresh SQL and close/reopen checks pass.

```sh
bash v6/labs/exec_shootout/postgres_pglite_ivm/13_crossover_run.sh take2 /tmp/sprefa-sqlite-native-take2/sweep.jsonl --take2-extension /tmp/sprefa-sqlite-native-take2/gate-ae3bh2ij/take2.dylib
IVM_RUN_ROOT=/tmp/sprefa-sqlite-native-take2/shared-semantic node v6/labs/exec_shootout/postgres_pglite_ivm/12_crossover_runner.mjs --profile semantic --arms sqlite-native-take2,sqlite-native-take2-logged --take2-extension /tmp/sprefa-sqlite-native-take2/gate-ae3bh2ij/take2.dylib --output /tmp/sprefa-sqlite-native-take2/shared-semantic.jsonl --warmups 0 --repetitions 1
/opt/homebrew/bin/python3 v6/labs/exec_shootout/postgres_pglite_ivm/42_sqlite_native_take2/8_receipts.py /tmp/sprefa-sqlite-native-take2/sweep.jsonl /tmp/sprefa-sqlite-native-take2/sweep.artifacts
```

Median cumulative mutation-plus-query milliseconds per case (two repetitions):

| Rows | Batch | Fanout | Unlogged | Logged |
|---:|---:|---:|---:|---:|
| 400 | 10 | 10 | 3.304 | 3.186 |
| 400 | 10 | 200 | 2.747 | 2.979 |
| 1200 | 10 | 200 | 3.009 | 3.335 |
| 4000 | 10 | 200 | 3.244 | 3.706 |
| 12000 | 1 | 200 | 2.168 | 2.301 |
| 12000 | 10 | 10 | 4.030 | 4.922 |
| 12000 | 10 | 200 | 4.339 | 5.529 |
| 12000 | 100 | 200 | 25.609 | 24.421 |
| 12000 | 1000 | 200 | 217.814 | 203.264 |

The 256-event log cap can stop logging before a large case ends. Counter work is
enabled in both modes. Two repetitions do not establish a logging-overhead ratio.
Setup includes incremental initial loading and is reported separately. Receipts
retain process RSS scope, setup/mutation/query timings, live DB/WAL/SHM bytes and
post-close DB bytes. No total-memory hard cap or cross-engine performance result.

Current gaps: no batching,
arbitrary SQL compiler, negation, cyclic recursive retraction or DD time/frontier
support is claimed. Shared circuit adapters remain unsupported, including catalog
self/chain shapes distinct from the local equal-key grouped-product families.
Exact callback timelines are asserted for autocommit, nested savepoints, source
conflict modes and xSync failure; other cases assert result/rollback contracts.
PG, DD, SWI and Take 1 were preserved but not executed in this worktree. No local
PostgreSQL/pg_ivm installation or pg node package was present for combined runs.

No changes to Take 1, prior templates, compiler/kernel or other worktrees.
