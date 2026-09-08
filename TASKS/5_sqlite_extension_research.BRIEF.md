# SQLite IVM extension boundary research

User requested Terra to fetch and study relevant repositories and SQLite FTS5,
then explain whether OpenIVM compilation can combine with FTS5 architecture.
Use gpt-5.6-terra medium. Research only. Astra IVM is paused; do not resume it.

## Scope and preservation

Work only in this new research worktree and task-owned temporary clone directories.
Base implementation is f7a7f937f. Read AGENTS.md and this brief first. Preserve all
other worktrees, processes, dirty edits, databases, configuration and installed CLIs.
No compiler/kernel changes, plugin rewrite, global installs, merges or pushes.
No native subagents or boop wait. Use boop for milestone hails to
sprefa-ivm-extract-parent. No nested agents. Commit research in bounded steps.
Use apply_patch for authored files. No subjective build-versus-buy judgments.

## Acquire source

Clone into a fresh mktemp directory, or fetch into task-owned clones only. Record
URL, full commit SHA, license, default branch, local absolute path, and retrieval
date. Do not commit vendor trees or build products. Fetch these source families:

- https://github.com/sqlite/sqlite : ext/fts5 source and transaction tests,
  virtual table implementation, session/preupdate, VDBE transaction callbacks.
- https://github.com/sraoss/pg_ivm : transition-table delta rewriting and tests.
- https://github.com/cwida/ivm-extension and https://github.com/ila/openivm :
  establish lineage and actual default branches; do not conflate repositories.
- https://github.com/vlcn-io/cr-sqlite and https://github.com/vlcn-io/materialite :
  trace https://github.com/vlcn-io/cr-sqlite/discussions/309 to implementation.
  That follow-up proposes SQLite hooks feeding Feldera-generated Rust; distinguish
  proposal, implemented code, and verified test coverage.
- Follow Rindle's https://rindle.sh/docs/how-it-works?path=engine to its actual
  public repository if available. Identify SQL-resident vs external engine state.
- Follow directly relevant source links to Feldera, GRDB or other implementations
  selectively where needed to verify an assertion. Avoid an unlimited survey.

## Questions with source evidence

1. FTS5 xUpdate/xBegin/xSync/xCommit/xRollback/xSavepoint/xRelease/xRollbackTo:
   callback order, when nested SQL is legal, shadow table writes, pending memory,
   visibility before COMMIT, savepoint failure, statement rollback and conflicts.
   Inspect actual tests as well as comments. xSync is not a statement-end hook.
2. Can ordinary source tables forward OLD/NEW to an extension virtual table via
   minimal triggers, preserving constraints and allowing batching? Which writer
   connections must load the module? What happens without it? How to avoid hook
   ownership conflicts? Explain direct vtab sources versus forwarded changes.
3. What public APIs expose query structure? Distinguish SQL parsing/binding,
   planner constraints, VDBE inspection, internal unsupported APIs and forks.
4. OpenIVM: exact reusable compiler code and dependencies on DuckDB parser,
   logical plans, optimizer/executor, SQL dialect/MERGE and refresh scheduling.
   Which operators truly use deltas versus affected-key or full recomputation?
   NULL/bags/DISTINCT/outer/self/multiway/recursive semantics and tests.
5. Compare existing local 25_sqlite_ivm implementation and compiler SQL templates.
   Identify what can be retained without implementing changes. No assumptions
   that the DD circuit suite means the SQLite plugin supports those circuits.
6. Does OpenIVM plus FTS5-style lifecycle/storage compose? List proved components,
   missing glue, incompatible contracts and unanswered questions. Keep query
   incrementalization distinct from transaction scheduling and persistent state.

## Deliverables

Create numeric-prefixed research files under
v6/labs/exec_shootout/postgres_pglite_ivm/41_extension_research/:
0_sources.md pinned repository inventory and source/test permalinks;
1_sqlite_boundaries.md callback signatures and concrete timelines;
2_compiler_reuse.md dependency/operator compatibility evidence;
3_options.md concise candidate compositions and discriminating next experiments.
Include absolute local reading paths in dependency order and immutable upstream
links. Explain in short pseudo-TS, with C signatures for SQLite ABI. Use specific
before/after transaction examples. No giant prose dump for the human.

Run existing relevant upstream tests if feasible with bounded commands and
task-local databases, no global state. Maximum two full suite runs per selected
upstream package; prioritize focused transaction tests. Do not build all engines
or run huge performance sweeps. Report actual commands, exit codes, tested
revisions and skips. If execution needs new implementation, specify the smallest
future probe and leave it pending for review. No new production code.

Final handoff: commits, repository inventory, verified versus untested findings,
and a short answer to the user's composition question. Report acquisition and
first concrete source-reading progress before the final report.
