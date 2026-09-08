# SQLite Native Take 2

Additive public-ABI experiment. Take 1 and all prior arms remain unchanged.
SQLite owns source, shadow state and rollback. The extension owns synchronous
per-event maintenance. No external engine, hooks, subscriber replacement or read flush.

Reading order: `0_README.md`, `0a_state.h` (types), `0b_delta.h` (signed SQL
contributions and transport installer), `1_native.c` (ABI, storage, lifecycle),
`2_boundary.sql` (forwarding contract), `3_boundary_test.py` (SQL transport and
oracle), `4_gate.py` (bounded build/test receipts), `5_sources.md`,
`6_semantic_test.py` (query-family oracles).

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

`maintained_state(id,k,v,side,PRIMARY KEY(side,id)) WITHOUT ROWID` stores source
images, indexed by `(side,k)`. `maintained_stats(n)` counts successfully
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

No batching claim is made. Arbitrary SQL parsing, negation,
recursive cyclic retraction and DD time/frontiers are unsupported.

## Query-family delta maintenance

```sql
CREATE TABLE a(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);
CREATE TABLE b(id INTEGER PRIMARY KEY,k INTEGER,v INTEGER);
CREATE VIRTUAL TABLE result USING take2(join);
SELECT take2_attach('result','a',0,'id','k','v');
SELECT take2_attach('result','b',1,'id','k','v');
INSERT INTO a VALUES(1,7,3);
INSERT INTO b VALUES(1,7,5);
SELECT * FROM result; -- (7,1,15)
```

`take2_attach(vtab,source,side,id_column,key_column,value_column)` installs OLD/NEW
forwarding triggers in an internal savepoint. Source tables must be empty at
attachment; subsequent loading is ordinary INSERT. Column names are quoted with
SQLite's `%w`; values are bound. All sources and the virtual table use `main`.
Attach each source exactly once, using the side numbers below. Arbitrary source
schema changes are outside this lab contract.

| Mode | Source sides | Fresh-query meaning | Returned tuple |
|---|---|---|---|
| mirror (default) | 0 | source identity | id,k,v |
| filter | 0 | WHERE v>=0, GROUP BY k,v with bag count | k,v,n |
| bag | 0 | GROUP BY k,v with bag count | k,v,n |
| group | 0 | GROUP BY k, COUNT(*), SUM(v) | k,n,s |
| join | 0,1 | equijoin on k, grouped COUNT/SUM(a.v*b.v) | k,n,s |
| self | 0 | two occurrences of source 0 equijoined on k | k,n,s |
| multi | 0,1,2 | three sources equijoined on k, grouped COUNT/SUM(a.v*b.v*c.v) | k,n,s |

The vtab retains generic physical column names `id,k,v` for its three output
positions because those names also carry NEW event fields. Name result columns
through a SQL view when needed. Bag modes expose support counts, with one output
row per distinct projected tuple. Result cursor rowids are unique SQLite rowids.

`result_result(key TEXT PRIMARY KEY,k,v,n,s,nn)` stores signed accumulators.
`key` is SQLite JSON for the integer/NULL group key, including v in bag modes.
`nn` counts non-NULL contributions, so SQL SUM returns NULL for all-NULL groups.
Zero-support rows are deleted. Updates read and write only affected-key supports.
Mirror state is persisted in SQLite; no relation-sized native cache exists.

Per OLD event: remove its source image, then subtract its contributions against
remaining source state. Per NEW event: add contributions against existing source
state, then insert the image. Self-join expands the three nonempty occurrence
masks: Δ⋈R, R⋈Δ, Δ⋈Δ. On deletion, R denotes post-delete state and all three terms
are subtracted. Three-source joins bind the changed side to Δ and the others to
current persisted state; sequential events include cross terms as each image
becomes visible. The tests include multi-row changes and same-key duplicate bags.

The accepted value/key domain is NULL or integer in [-1000000,1000000]; identities
are SQLite integers. Accumulators must remain signed SQLite integers and preserve
support invariants. Overflow fails the source statement, including partial
multi-row progress. No floating-point or arbitrary expression contract is claimed.

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
