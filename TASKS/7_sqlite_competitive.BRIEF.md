# Competitive SQLite-native IVM continuation

Authorization received through parent sprefa-ivm-extract-parent, message
`m-1822a3bf`, 2026-09-08. Continue Astra medium in this worktree; no native children.

Preserve Take 1, Take 2 and every receipt. New experiment code and receipts live
under `v6/labs/exec_shootout/postgres_pglite_ivm/43_sqlite_competitive/` and
task-local scratch. Shared runner changes remain additive. Commit often; no
pushes, merges, DL7 kernel changes, global installs or user database mutations.

First obtain combined PG, pg_ivm, DD, SWI, Take 1 and Take 2 receipts through the
same shared Bash harness and exact oracle. Existing task-local binaries from
prior lanes may be read/reused or rebuilt task-locally. Profile the observed
Take 2 batch1000 cost before larger sweeps, then implement batching.

Maintain a finite option ledger with measured results or concrete source/test
rejections for: prepared-statement reuse and SQL/index tuning; bounded optional
session cache; delta consolidation; explicit SQL batch/flush API; vtab lifecycle
batching with precommit reads; preupdate/session capture with a supported drain
boundary; and a custom SQLite statement-boundary callback if public ABI cannot
supply the required boundary. Make no universal exhaustion claim.

Session-local state/configuration and task-local custom SQLite builds/patches
are authorized. Expose explicit SQL lifecycle contracts where necessary and
prove misuse fails rather than silently returning stale results. Retain stock
SQLite. A custom variant requires a pinned minimal patch, reproducible local
build, upstream transaction/savepoint/conflict tests and shared-oracle passes.

Broaden actual shared circuit support family by family: general filters,
self/multiway joins, negation, cyclic retraction and time/frontier variants where
feasible. Unsupported cases stay explicit. Full recomputation must not be labeled
incremental maintenance. Prefer reusable library/compiler units over a new parser
or framework. Preserve durable/volatile and consistency labels across all arms.

Report milestones via boop to the parent without waiting. Include commits, exact
commands, retained failures, current gaps and paired cross-engine statistics.
Parent independently reviews the prior gate while this work proceeds.
