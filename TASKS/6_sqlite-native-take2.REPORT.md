# SQLite Native Take 2

Current gate: PASS, 11 boundary tests and 6 query-family tests on SQLite 3.53.2.
Milestone 1 boundary gate at commit: PASS, 10 tests.
Upstream pinned SQLite FTS5: 3 savepoint tests and 27 conflict tests, zero errors.
Initial boundary attempt: exit 1, 9 passing tests and one incorrect expected
callback timeline. Focused repair: exit 0, 2 tests. Green gate: exit 0.
Logs retained under `42_sqlite_native_take2/receipts/`; source/build/log SHA256 and
exact commands in `receipts/receipt.json`. No previous passing lifecycle result
was inferred from Terra's failed testfixture build.

Worktree base: `46e918dac`; task brief commit: `0aa441195`.
Milestone 1 commit: `176b45818`.

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

CI coverage added: standalone C extension compilation, 11 boundary tests and
6 query-family tests. The extra boundary case rejects illegal shadow-trigger
virtual-table reentry without source/state divergence.
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

Current gaps: shared shootout arm and performance receipts are next. No batching,
arbitrary SQL compiler, negation, cyclic recursive retraction or DD time/frontier
support is claimed. Exact callback timelines are asserted for autocommit and
nested savepoints; other lifecycle cases assert result/rollback contracts.

No changes to Take 1, prior templates, compiler/kernel or other worktrees.
