# SQLite Native Take 2

User authorizes Astra medium to implement an additive SQLite-native IVM experiment
using the collected OpenIVM/FTS5 research. Use gpt-6-astra medium. Take 1 remains
paused and preserved. Work ONLY in this new worktree and task-local scratch paths.
Read AGENTS.md, this brief, and 41_extension_research/0..3 first. Base 46e918dac.

## Scope

Preserve 25_sqlite_ivm, prior templates, tests, benchmarks, and all other worktrees.
Add Take 2 under v6/labs/exec_shootout/postgres_pglite_ivm/42_sqlite_native_take2/.
Use numeric dependency/reading-order filenames. Shared runner changes must be
additive. Do not resume old Astra or Terra lanes. No native subagents, recursive
delegation, global installations, merges, pushes, or compiler/kernel changes.
No app-emitter maintenance loop: the extension owns maintenance, SQLite owns
persistent state. Do not introduce an external resident engine as a substitute.

## Evidence corrections and sources

Terra's research is source inspection, not passed lifecycle tests: testfixture
build failed for missing Tcl configuration. Find an existing Tcl installation
or use task-local build dependencies if needed; no global package mutation.
Its statement that discussion 309 does not name Feldera missed the replies:
https://github.com/vlcn-io/cr-sqlite/discussions/309 explicitly proposes hooks
feeding Feldera-generated Rust. Treat that as a proposal, not a SQLite-native
implementation. Do not blindly inherit every report conclusion.
Pinned upstream clones live at /tmp/sprefa-sqlite-extension-research.ZsALrZ/.
Read SQLite ext/fts5 callbacks, storage and tests plus src/vtab.c; OpenIVM delta
compiler/operator dispatch and tests; pg_ivm transition/delta logic. Retain SHAs
and licenses. Reuse available libraries and mechanisms; no subjective build/buy
claims. OpenIVM uses DuckDB-bound plans and SQL; no ready SQLite port is proven.

## Milestone 1: executable integration boundary

Build a minimal loadable extension using SQLite's public virtual-table ABI,
following FTS5-style shadow storage and transaction lifecycle. Reuse existing ABI
bindings where possible; C or Rust with C ABI are allowed. No SQLite fork without
review. Ordinary source-table forwarding triggers should send OLD/NEW changes
into a writable vtab; probe direct vtab sources only as a separately labeled case.

Prove callback order with bounded structured traces and exact assertions:
- multi-row writes, autocommit and explicit transactions;
- read-your-writes after every completed statement and before COMMIT;
- nested savepoints, release, rollback-to, outer rollback;
- statement failure after partial progress, ABORT/FAIL/IGNORE/REPLACE/UPSERT and
  constraint failures with stated contracts, no silent stale results;
- xSync failure rolls back source and maintained state;
- close/reopen, second writer connection, module present/absent, no silent gap;
- shadow-state consistency and no illegal SQL callback reentrancy.
xSync is transaction preparation, not a general statement-end callback. A read
flush strategy must prove legality and exact visibility. If batching cannot meet
the contract, document the observed limitation and retain the working path;
do not weaken correctness or present a full recompute as delta maintenance.
Persist test logs including failures, then green receipts. Commit this milestone.

## Milestone 2: actual incremental maintenance

After boundary proof, reuse Take 1 algorithms/templates and upstream compiler
techniques to implement supported query families in Take 2. Begin with filter/
projection, bag support counts, join and grouped COUNT/SUM; then self/multiway
join with correct cross terms. Each family needs red shared-oracle cases then
green implementation. Keep SQL parsing/binding, delta generation, transaction
scheduling and storage distinct without inventing a large framework.
Before an extensive OpenIVM compiler port, document the concrete reusable unit,
dependency mapping and smallest compiling target. No wholesale compiler rebuild
just to chase parity. Extend incrementally where evidence supports it.
Negation, recursive cyclic retraction and DD time/frontiers remain explicit later
families: do not claim solved from FTS5/OpenIVM or rejection-only tests. Do not
change DL7 kernel contracts; surface any required changes for user education and
approval. Continue safe independent milestone work when a family is blocked.

## Milestone 3: same shootout and telemetry

Add a separate sqlite-native-take2 arm to existing 13_crossover_run.sh/shared
runner, preserving Take 1, PG-IVM, DD, SWI and full-query arms. Same exact input/
output oracle first, then bounded repeated size/batch/fanout/churn sweeps. Reuse
memory-pressure guards and deadlines. Keep durable SQL versus volatile DD labels,
setup/mutation/query timings, RSS scope, DB/WAL bytes and unsupported cells.
Logging: scoped bounded stderr, no global callback/subscriber replacement, no
payload leakage; transaction-safe counters and separate logged/unlogged costs.
No algorithm in Python/JS transport. No unbounded memory copies of all relations.

## Delivery

Commit each tested step often, no pushes. Use apply_patch for source edits.
Run cargo fmt once before Rust commits; do not review formatter churn. At most
four full gates per milestone; use focused tests for repairs. All databases and
build targets task-owned, leave user data and processes untouched. Define one
reproducible Take 2 gate and preserve its exit codes, hashes and receipts.
Maintain 42_sqlite_native_take2/0_README.md with API signatures, concrete SQL,
callback timeline, storage layout, proven/unsupported behavior and reading order.
Write TASKS/6_sqlite-native-take2.REPORT.md with commits, current test counts,
CI coverage changes, exact run commands, performance receipts, remaining gaps.
Do not stop at planning: execute milestone 1 and proceed through feasible scoped
milestones. Report source acquisition/start and each milestone through boop to
sprefa-ivm-extract-parent. No boop wait. Parent owns sampled review and integration.
