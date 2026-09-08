# SQLite IVM plugin milestone, 2026-09-08

Lane: feature/sqlite-ivm-astra. Scope remains in progress: executable slice and
telemetry implemented; shared shootout integration and semantic inventory next.
No compiler/kernel changes, worker delegation, merge, push or global install.

Build succeeds with pinned rusqlite 0.40.2 and sqlite3-parser 0.17.0.
19/19 loaded-extension tests pass, including 240 seeded transitions, 32 explicit
query rejection cases, stock CLI .load, duplicate supports on both join sides,
conflict policies, second writers without extension loading, failed install
cleanup, caller rollback, integer cap rejection, and diagnostic sink failure.
Coverage added: executable local build/test coverage; no CI workflow changed.

Absolute component path:
/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-astra/v6/labs/exec_shootout/postgres_pglite_ivm/25_sqlite_ivm/

Extension: /private/tmp/sqlite-ivm-astra-target/debug/libsqlite_ivm.dylib
CLI: /opt/homebrew/opt/sqlite/bin/sqlite3, SQLite 3.53.2.
Public signatures, state lifetime, reading order, dependency/license audit and
telemetry limitations: 25_sqlite_ivm/0_README.md.
Public result is a read-only SQL view over a maintained ordinary table.

The complete finite semantic matrix, shared grid receipts, logging overhead and
final commit inventory will replace this milestone report during handoff.
