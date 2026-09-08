# Composition candidates and discriminating probes

Read after [2_compiler_reuse.md](2_compiler_reuse.md).

## Components by contract

| Component | Proved by source | Missing contract for composition |
|---|---|---|
| SQLite trigger transport | OLD/NEW values, source constraints, statement and transaction rollback propagate to trigger writes | one batch boundary across many trigger rows |
| SQLite vtab lifecycle | a vtab can own transaction-local pending state and flush/fail at `xSync`; FTS5 handles savepoints | ordinary source tables do not automatically enroll an unrelated vtab |
| FTS5 storage pattern | shadow tables plus pending memory participate in the writer transaction | no generic query delta compiler |
| Local SQLite IVM templates | covered inner join/count/sum state can be stored and maintained by persistent triggers | arbitrary SQL parser/binder and general semantic matrix |
| OpenIVM | DuckDB-bound query classification and many delta/recompute strategies | SQLite parser/planner/catalog/dialect and transaction integration |
| pg_ivm | statement transition-table delta rewriting | SQLite has no equivalent public transition-table trigger API |

## Candidate compositions

### A. SQLite-resident trigger templates

```ts
on source row trigger:
  write OLD/NEW to persistent ivm_change_queue
  update covered result/support tables in the same SQLite statement

on COMMIT:
  SQLite atomically publishes source + result + queue changes
```

This is the existing local transport. It shares statement constraints and rollback with sources. Row-trigger invocation means a multi-row statement can recompute the same affected group multiple times. A persistent queue can retain events for later work, but no general once-per-transaction drain is proved by FTS5 callbacks because the IVM source tables are ordinary tables, not its vtab.

### B. Forwarded writable vtab with FTS5-style lifecycle

```ts
source AFTER trigger -> INSERT ivm_vtab(OLD, NEW)
ivm_vtab.xUpdate -> pending in-memory delta + shadow/state writes
ivm_vtab.xSync -> flush work that may fail
ivm_vtab.xRollback/xRollbackTo -> discard pending state
```

This combines SQLite lifecycle and source forwarding when every writer registers the module. It requires explicit implementation of vtab state, savepoint depth, queue/result schema, and no SQL reentrancy violation in `xSync`. It does not provide OpenIVM compilation.

### C. External OpenIVM/Feldera-style processor consuming committed queue rows

```ts
SQLite transaction -> durable source + change queue
after commit, external worker -> read committed queue -> apply external circuit -> checkpoint/output
```

The pinned discussion proposes observation work and mentions Rust/WASM direction, but no Feldera source integration or test coverage was found in the two pinned vlcn repositories. Durable queue acknowledgement, crash ordering, read visibility, and materialized-output ownership remain unverified. This has external engine state unless the external circuit state is checkpointed into SQLite under an explicit protocol.

### D. OpenIVM compiler port targeting SQLite state templates

```ts
SQLite SQL text -> independent parse/bind/catalog -> compatible logical IR
IR -> classify: algebraic delta | affected-key recompute | full refresh/reject
IR -> SQLite INSERT/UPDATE/DELETE trigger templates + persistent auxiliary tables
```

OpenIVM contains useful classification vocabulary and test cases, but its implemented compiler is DuckDB-specific. Required translation points are SQLite SQL syntax and NULL/bag behavior, absence of `MERGE`, source-row versus statement-delta transport, persistent auxiliary state, and a scheduler policy separate from compilation.

## Composition answer

OpenIVM compilation and FTS5-style storage/lifecycle can compose only across a new adapter boundary: a SQLite-compatible logical IR/compiler target must emit SQLite state and trigger/vtab operations; a source forwarding path must supply changes; and a transaction owner must define flush and rollback behavior. FTS5 provides evidence that SQLite vtab pending state and shadow writes can be transaction/savepoint-correct. OpenIVM provides DuckDB-specific delta and recompute code. Neither repository supplies the adapter.

## Smallest future probes, pending review

| Probe | Fixed input | Pass observation | Unanswered if it passes |
|---|---|---|---|
| vtab lifecycle trace | one vtab, one ordinary source trigger, `BEGIN;` two writes; savepoint rollback; commit | callback log establishes exact `xBegin/xUpdate/xSavepoint/xRollbackTo/xSync/xCommit` order and persisted row counts | compilation and general SQL semantics |
| writer-module enforcement | second SQLite connection executes source trigger before/after module load | exact prepare/write result, no silent maintenance gap | multi-process module deployment policy |
| affected-group batching | one multi-row `INSERT ... SELECT` touching one group vs row-by-row insert | count result writes and exact final result | transaction-level batch drain mechanism |
| SQL semantic compatibility | fixed query corpus: NULL, duplicate bags, DISTINCT, left/full/self/multiway joins, recursion | SQLite fresh query equals maintained state after insert/update/delete/savepoint rollback | a general parser/binder/compiler |
| durable external queue | committed queue rows, worker crash before/after checkpoint | recoverable exactly-once or documented at-least-once output | external circuit semantics and scheduling |

No production code is added by this research.
