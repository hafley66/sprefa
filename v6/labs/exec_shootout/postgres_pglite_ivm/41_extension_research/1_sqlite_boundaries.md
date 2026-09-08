# SQLite extension transaction boundary

Read after [0_sources.md](0_sources.md). Local source order: `/tmp/sprefa-sqlite-extension-research.ZsALrZ/sqlite/src/sqlite.h.in`, then `src/vtab.c`, then `src/vdbe.c`, then `ext/fts5/fts5_main.c`, then `ext/fts5/test/fts5savepoint.test`.

## ABI and core dispatch

```c
int (*xUpdate)(sqlite3_vtab *, int argc, sqlite3_value **argv, sqlite3_int64 *pRowid);
int (*xBegin)(sqlite3_vtab *);
int (*xSync)(sqlite3_vtab *);
int (*xCommit)(sqlite3_vtab *);
int (*xRollback)(sqlite3_vtab *);
int (*xSavepoint)(sqlite3_vtab *, int i);
int (*xRelease)(sqlite3_vtab *, int i);
int (*xRollbackTo)(sqlite3_vtab *, int i);
```

The public layout is in [`sqlite.h.in:7735`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/src/sqlite.h.in#L7735-L7768). A vtab joins `db->aVTrans` only after successful `xBegin`; if a statement begins inside existing savepoints, core immediately invokes `xSavepoint` for the current depth ([`vtab.c:1055`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/src/vtab.c#L1055-L1088)). Core iterates all enrolled vtabs for sync, finalization, and savepoint calls ([`vtab.c:967`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/src/vtab.c#L967-L1139)).

`xSync` is the first phase of transaction commit, not an end-of-DML-statement callback. A transaction may contain any number of statements and nested savepoints before it runs. Core stops at the first `xSync` error. The FTS5 comment records that current SQLite ignores errors returned from `xCommit` ([`fts5_main.c:40`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_main.c#L40-L74)).

SQLite permits nested SQL issued by a vtab only under its normal virtual-table reentrancy restrictions. In particular core rejects beginning a vtab transaction while it is in `sqlite3VtabInSync` ([`vtab.c:1048`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/src/vtab.c#L1048-L1058)). A flush/scheduling design must avoid using `xSync` as an arbitrary SQL execution point.

## FTS5 implementation facts

| Callback | FTS5 action | Persistent state / failure boundary |
|---|---|---|
| `xUpdate` | Writes FTS5 content/docsize/index shadow tables through prepared SQL and accumulates index terms. | It receives each source row separately. `sqlite3_vtab_on_conflict()` and core statement rollback govern conflicts. |
| `xBegin` | Starts FTS5 transaction state. | Enrolls the vtab only after success. |
| `xSync` | `sqlite3Fts5FlushToDisk()`. | Pending terms become shadow-table writes before database commit; error can abort commit. |
| `xCommit` | No-op. | Pending terms were flushed by `xSync`. |
| `xRollback` | Drops pending terms via `sqlite3Fts5StorageRollback`; SQLite reverts shadow-table writes. | Clears cached page size. |
| `xSavepoint` | Flushes pending terms and records savepoint depth. | A flush error fails savepoint opening. |
| `xRelease` | Flushes only while releasing an outer active depth. | A flush error fails release. |
| `xRollbackTo` | Trips cursors and discards pending terms if its saved depth applies. | SQLite rolls shadow-table pages back to the named savepoint. |

Evidence: [`fts5_main.c:2117-2164`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_main.c#L2117-L2164), [`fts5_main.c:3158-3215`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_main.c#L3158-L3215), and storage DDL / writes in [`fts5_storage.c`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/fts5_storage.c).

Before outer `COMMIT`, the writer connection sees its own source and FTS5 shadow-table writes according to ordinary SQLite transaction visibility; another connection does not see uncommitted writes. Pending terms that have not yet been flushed are FTS5 memory state. FTS5 flushes them during `xSync`, savepoint open, and selected release, so `xSync` has no statement batching meaning.

The shipped savepoint test inserts `a`, `b`, `c`, inserts then rolls back `d`, and commits `a b c`; it also tests a shadow-index damage error across two FTS5 tables ([`fts5savepoint.test:22-55`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/test/fts5savepoint.test#L22-L55)). `fts5conflict.test` checks `OR IGNORE` preserves a checksum on duplicate rowid cases ([`fts5conflict.test`](https://github.com/sqlite/sqlite/blob/f3b9f74d81132426dee1ccc07a67fdad2ccfeaa9/ext/fts5/test/fts5conflict.test)). The focused upstream run is blocked as recorded in `0_sources.md`.

## Forwarding ordinary tables to an extension vtab

```sql
CREATE TRIGGER source_ai AFTER INSERT ON source BEGIN
  INSERT INTO ivm_changes(view_name, op, old_json, new_json)
  VALUES ('v', 'I', NULL, json_object('id', NEW.id, 'x', NEW.x));
END;
CREATE TRIGGER source_au AFTER UPDATE ON source BEGIN
  INSERT INTO ivm_changes(view_name, op, old_json, new_json)
  VALUES ('v', 'U', json_object('id', OLD.id, 'x', OLD.x), json_object('id', NEW.id, 'x', NEW.x));
END;
CREATE TRIGGER source_ad AFTER DELETE ON source BEGIN
  INSERT INTO ivm_changes(view_name, op, old_json, new_json)
  VALUES ('v', 'D', json_object('id', OLD.id, 'x', OLD.x), NULL);
END;
```

These minimal row triggers run in the source statement and therefore retain source constraints, `OR ABORT` statement rollback, savepoint rollback, and outer transaction rollback. They can forward directly by inserting into a vtab, or stage durable rows in an ordinary queue table. SQLite triggers are row-level, so the first form calls the target once per row; the queue form permits a future transaction-level vtab drain only if a lifecycle owner is available.

Every writer connection that executes a trigger referring to the module must load and register that module before executing the write. Without it, statement preparation/execution fails with `no such module` or missing function/table resolution; it does not silently maintain a different target. Direct vtab sources receive `xUpdate` arguments and own their transaction callbacks. Forwarded ordinary sources keep source writes in core tables, while trigger SQL creates the target writes; their extension has no observation hook ownership.

Avoid `sqlite3_update_hook` / `sqlite3_preupdate_hook` for this persistent maintenance path. Each hook family has one callback slot per connection, sees only that connection, and requires connection-owned buffering plus commit/rollback handling. Discussion 309 records those limits. Trigger-owned forwarding avoids that callback ownership conflict, but it needs installed persistent trigger definitions and the module on all writer connections.

### Timelines

```ts
// successful source statement under outer transaction
BEGIN;
UPDATE source SET x = 2 WHERE id = 7;
// per affected row: source constraint checks -> source_au sees OLD/NEW -> queue/vtab write
SAVEPOINT s;
INSERT INTO source VALUES (8, 3);
ROLLBACK TO s;       // removes source and queue/vtab effects after xRollbackTo
COMMIT;              // xSync may flush; then xCommit finalizes

// statement conflict
UPDATE OR ABORT source SET unique_x = 2; // later row conflicts
// SQLite statement rollback removes all source-trigger forwarding from that statement
```

An FTS5-like vtab can keep per-transaction memory, flush at `xSync`, and discard at rollback. It cannot infer changes in ordinary source tables unless a trigger writes to it or a connection-owned hook observes them.

## Query-structure APIs

| Need | Public SQLite surface | Scope |
|---|---|---|
| Parse, bind, resolve names/types | `sqlite3_prepare_v3`, bind APIs, `sqlite3_column_*`, `sqlite3_expanded_sql` | Public but exposes a prepared statement, not a public AST. |
| Planner constraints for a vtab scan | `sqlite3_index_info` in `xBestIndex` | Constraints presented for that vtab access only. |
| Read dependencies during prepare | `sqlite3_set_authorizer` | Public callback reports table/column access, as used by the discussion proposal. |
| Observe row values | `sqlite3_preupdate_hook` if SQLite was compiled with `SQLITE_ENABLE_PREUPDATE_HOOK` | Public conditional API; connection-local and single callback slot. |
| VDBE program | `EXPLAIN` SQL text / rows | Public diagnostic output, no supported stable opcode API. |
| Internal parse tree, planner, VDBE objects | `Parse`, `Select`, `Expr`, `Vdbe`, `sqlite3Vdbe*` | Private SQLite source interfaces, unavailable to normal loadable extensions and version-coupled in a fork. |

The public vtab ABI accepts SQL-selected constraints through `xBestIndex`; it does not expose the full caller query AST or bound logical plan. A general SQL IVM compiler therefore needs an independent parser/binder/catalog layer, a supported engine integration, or a maintained SQLite fork.
