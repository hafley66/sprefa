# SQLite Native Take 2

Additive public-ABI experiment. Take 1 and all prior arms remain unchanged.
SQLite owns source, shadow state and rollback. The extension owns synchronous
per-event maintenance. No external engine, hooks, subscriber replacement or read flush.

Reading order: `0_README.md`, `1_native.c` (ABI, storage, lifecycle),
`2_boundary.sql` (forwarding contract), `3_boundary_test.py` (SQL transport and
oracle), `4_gate.py` (bounded build/test receipts), `5_sources.md`.

## API and SQL

```c
int sqlite3_extension_init(sqlite3 *, char **, const sqlite3_api_routines *);
int xUpdate(sqlite3_vtab *, int argc, sqlite3_value **argv, sqlite3_int64 *);
int xBegin/xSync/xCommit/xRollback(sqlite3_vtab *);
int xSavepoint/xRelease/xRollbackTo(sqlite3_vtab *, int depth);
```

```sql
PRAGMA recursive_triggers=ON; -- enforced for every event, including second writers
CREATE VIRTUAL TABLE maintained USING take2;
INSERT INTO maintained(id,k,v,op) VALUES(NEW.id,NEW.k,NEW.v,1);
INSERT INTO maintained(op,old_id,old_k,old_v) VALUES(2,OLD.id,OLD.k,OLD.v);
INSERT INTO maintained(id,k,v,op,old_id,old_k,old_v)
VALUES(NEW.id,NEW.k,NEW.v,3,OLD.id,OLD.k,OLD.v);
SELECT id,k,v FROM maintained;
SELECT take2_control('trace'); -- drains bounded diagnostic JSON, no row values
SELECT take2_control('fail_sync'); -- one-shot connection-local fault injection
```

The INSERT fragments belong in ordinary AFTER triggers, as installed by
`2_boundary.sql`. They carry events only. Direct event INSERT is separately tested;
direct UPDATE/DELETE on the virtual table are unsupported.

## Instance timeline and storage

One module environment per connection owns a 16 KiB diagnostic buffer, capped at
256 events per drain, logging switch and one-shot test fault. Each connected vtab
owns database/name strings and a reentrancy guard. Each scan owns one SQLite
statement until cursor close. No relations are copied into extension memory.

`maintained_state(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER)` is the mirror used
to establish the integration boundary. `maintained_stats(n)` counts successfully
applied events within SQLite transactions. Both are recognized by `xShadowName`.
Trusted schema ownership is required: DDL/trigger removal and direct shadow edits
are outside the installed-maintenance contract. OLD mismatches fail explicitly.

For the observed two-row autocommit INSERT on SQLite 3.53.2:

```text
begin
update(ABORT), savepoint(0), release(0)
update(ABORT), savepoint(0), release(0)
sync, commit
```

Nested shadow INSERTs cause nested statement savepoints during xUpdate. All
nested DML uses `SQLITE_PREPARE_NO_VTAB`. The lifecycle callbacks perform no SQL.
xFilter performs a read-only scan of ordinary storage. SQLite restores shadow
pages and counters on statement/savepoint/transaction rollback; there is no
pending data cache to restore. xSync is transaction preparation and can fail.

## Contracts under test

Completed statements expose source-equal shadow state before COMMIT. ABORT rolls
back the failing statement; FAIL preserves its completed prefix; IGNORE skips
conflicting source rows; REPLACE forwards its deletion with recursive triggers
enabled; UPSERT forwards UPDATE. Source CHECK failure aborts the statement.
Missing module rejects source writes. A second loaded connection obeys SQLite
writer locking and sees committed state. An xSync error rolls back both state and
source. Diagnostics describe attempts, while SQL counters roll back with data.

No batching claim is made. Boundary state is an identity projection; query-family
delta maintenance is a subsequent milestone. Arbitrary SQL parsing, negation,
recursive cyclic retraction and DD time/frontiers are unsupported.

## Gate

```sh
/opt/homebrew/bin/python3 v6/labs/exec_shootout/postgres_pglite_ivm/42_sqlite_native_take2/4_gate.py
```

Override `TAKE2_SCRATCH`, `TAKE2_SQLITE_INCLUDE` for local paths. The gate creates
a unique task-owned run directory, compiles with public headers, runs bounded
tests and saves child exit codes, commands, timings, source/build/log hashes.
Failures preserve their exit codes. Python transports SQL and compares results.

Upstream SQLite testfixture uses an isolated archive of the research pin, built
with `--enable-fts5 --with-tcl=/opt/homebrew/opt/tcl-tk@8/lib`. The local Python
driver uses SQLite 3.53.2; its boundary result is distinct from upstream FTS5 tests.
