# Postgres and PGlite IVM shootout

User explicitly requests a Sol high worktree to investigate and RUN native
Postgres and PGlite in the existing IVM/class-of-problem scale shootouts.
Use the codex sol preset (gpt-5.6-sol, high). Work only in your assigned
worktree, never change the primary checkout. Commit progress in logical chunks.

## Locate and reuse

Read local AGENTS.md. Compiler kernel changes require user explanation and
approval: none are authorized here. Do not use v6/dd-runner as design input.
Start with v6/labs/exec_shootout/CONTRACT.md, STANDINGS.md, harness/, and
tick_bench/TICKS.md; also v7/labs/18_runtime_shootout/0_README.md and 4_run.sh.
Search for any broader existing IVM workload battery before adding fixtures.
Reuse generators, correctness oracles, receipt shapes and reporting entrypoints.
The old tick_bench includes process-per-tick recomputation: label it accurately.
Existing contracts may prohibit naive recomputation; add a separately labeled
comparison category instead of silently weakening that contract.

## Implement and run

1. Inventory installed Postgres, pg_config, pg_ivm, Node and PGlite. Verify
   actual package/extension versions and use official docs. PGlite currently
   lists pg_ivm at https://pglite.dev/extensions/#pg_ivm . Verify by loading it.
2. Use isolated temporary database clusters, loopback/private sockets and
   disposable PGlite directories. Never connect to or alter user databases,
   system services, shared daemons or global configurations. Use task-local
   dependencies; request approval if installation needs broader permissions.
3. Add native Postgres and PGlite entrants. Distinguish ordinary query/full
   recomputation, native pg_ivm and WASM pg_ivm. Optionally measure PGlite
   live.changes/incrementalQuery separately: they rerun and diff results and
   are not evidence of incremental SQL evaluation.
4. Run existing compatible problem families. Cover join/fanout, grouped
   count/sum, inserts, deletes, duplicate support and batched updates. If the
   existing suite lacks nonrecursive IVM cases, add the smallest shared fixture
   family and run both new implementations against the same reference.
   pg_ivm does not support WITH RECURSIVE: mark unsupported, and benchmark
   recursive full-query evaluation separately where existing contracts allow.
5. Use the existing size ladder where safe, plus bounded smoke cases. Measure
   size AND update-batch size. Start small; stop larger cases at explicit time
   and memory bounds with timeout/unsupported/error rows, never fabricate zero.
   Avoid parallel heavy benchmark processes. Per-case timeout at most 120s;
   keep generated results bounded and no corpus data committed.
6. Check exact output equivalence/checksums after each update, not just counts.
   Report setup/load/index/view build, update transaction incl maintenance,
   query/readback, client transfer, wall time and memory separately. Include
   server/backend memory for native Postgres, JS/WASM memory for PGlite; state
   measurement scope. Record durability, indexes, versions, hardware, seeds,
   warmups/repetitions. Keep raw machine-readable receipts and commands.
7. Integrate an opt-in runner and deterministic smoke correctness gate into
   existing shootout entrypoints. Preserve normal dependency-free defaults.
   Report CI coverage added/changed and current executed results.

## Deliverable and boundaries

TASKS/1_postgres-pglite-ivm.REPORT.md: discovered harness, arms, supported
workload matrix, scaling receipts, correctness, commits, exact rerun commands,
and blocked/skipped cases with reasons. Provide primary-source links.
Do not claim performance superiority without comparable runs. No new compiler
semantics, migration, harness redesign, or production database deployment.
Use a unique CARGO_TARGET_DIR when invoking Cargo. Do not push or merge your
branch; parent reviews integration. Read boop --help for completion protocol.
