# SQLite IVM plugin integration milestone, 2026-09-08

Lane: feature/sqlite-ivm-astra. Executable plugin, shared shootout integration,
finite semantic matrix and telemetry are implemented. Final binary performance
receipts and complete artifact/provenance handoff remain in progress.

Commit b1bba8133 contains the initial loaded extension milestone. This milestone
adds shared plugin arms, 171-state semantic fixture, generalized matching reports,
logging overhead tables, source-backed matrix, and catalog foreign-key rejection.
No compiler/kernel changes, worker delegation, merge, push or global install.

Current complete gate: 54/54 tests passed at
v6/labs/exec_shootout/postgres_pglite_ivm/results/plugin-20260908/final-gate-2/.
Counts: loaded extension 23, preserved SQLite template 6, SQLite mechanisms 10,
shared integration 5, store SQL references 6, store DD references 4.
Builds: release extension and actual DD example. Added local executable build/test
coverage, changed shared fixture coverage; no CI workflow was changed.

Five-arm semantic run: 171 states/arm, 855 exact input/output validations.
Initial repeated grid: 45 matched five-arm trials plus warmups, 1350 exact states,
zero failed/timeout/resource-blocked rows. These receipts preserve the binary
before the final foreign-key catalog rejection; a final binary rerun follows.
The negative-zero fixture fix deep-equals the executed serialized fixture;
m2-fixture-normalization.log records that comparison. The failing test is retained.

Absolute component path:
/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/25_sqlite_ivm/

Extension: /private/tmp/sqlite-ivm-astra-target/release/libsqlite_ivm.dylib
CLI: /opt/homebrew/opt/sqlite/bin/sqlite3, SQLite 3.53.2.
Public signatures, maintenance sequence, filesystem reading order, dependency
license audit and logging controls: 25_sqlite_ivm/0_README.md.
Finite supported/rejected/missing matrix: 28_semantic_coverage.md.
Repeatable complete gate: bash 29_plugin_gate.sh <fresh-receipt-directory>.
Shared benchmark entry remains 13_crossover_run.sh; new arms are
sqlite-plugin-delta and sqlite-plugin-logged. The affected-group arm is preserved.

General plugin breadth is incomplete. NULL/global/AVG/MIN/MAX/filter/set/outer/
self/multiway/recursive/time/window semantics are rejected or absent, as indexed
in the matrix. Public result is a read-only view over a maintained table. Internal
schema edits and C APIs bypassing SQL triggers remain outside ordinary DML.
Foreign-key cascades and existing unmanaged source triggers reject at install.
Telemetry does not intercept host trace/commit/rollback callbacks. Lifecycle logs
are connection-local; fixed-size transactional counters include other writers.
