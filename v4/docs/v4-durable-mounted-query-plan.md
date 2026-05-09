# V4 Durable Mounted Query Plan

## Scope

Foundation includes:

```text
effect_runtime
sprefa core
store
```

The foundation slice now has durable queue state and durable mounted SQL output state. The remaining mounted-query work is narrower: make more state survive process restart, shrink dirty matching, and move any remaining component-local buffers into store-backed scopes when needed.

## Current State

Implemented:

- batched dispatch through `effect_runtime`
- parked rows and wake through `Yield` / `next`
- barrier lifecycle: `dispatch`, `idle`, `complete`
- `collect()` and `collect_ready(...)`
- rule declaration, writes, reads, predicates, and SQL batch-local query op
- fact table row identity through `_id`
- dirty row publish exists through `FactStore::commit`
- SQL query outputs are persisted through `mounted_query_output` and
  `mounted_query_cursor`
- SQL mounts record dependencies in `mounted_query_dep`
- mounted SQL parks continuations on referenced table dirty keys
- late writes to referenced rule tables wake parked SQL continuations
- mounted query reruns diff outputs and emit only new output cursor hashes
- anti-join disappearance retracts downstream supported rule rows through
  `mounted_query_support`
- `SqliteQueue` serializes parked continuations and can revive them after
  reopen
- app drivers can use `--queue-db`, `--fact-db`, or both
- `SqliteFactStore` supports core/internal tables and mounted-query columns
  such as `_strings`, `generation`, and `__support_cursor_id`

Current limitations:

- `CollectComponent` buffers in a component `Mutex`, not durable store state
- mounted query definitions and outputs persist in the fact store, but query
  continuations are only durable when the queue backend is `SqliteQueue`
- mount recovery across a full process restart needs an integration driver that
  reopens both `SqliteFactStore` and `SqliteQueue`, then resumes parked rows
- dirty publish is table-level for mounted SQL dependencies
- stale output rows are replaced for a mount/input key; broader negative-diff
  propagation is limited to support rows recorded by rule writes

## Durable Tables

Minimum durable shape:

```sql
CREATE TABLE mounted_query (
  mount_id TEXT PRIMARY KEY,
  query_hash TEXT NOT NULL,
  query_sql TEXT NOT NULL,
  created_at_generation INTEGER NOT NULL
);

CREATE TABLE mounted_query_dep (
  mount_id TEXT NOT NULL,
  table_name TEXT NOT NULL,
  column_name TEXT,
  value TEXT
);

CREATE TABLE mounted_query_output (
  mount_id TEXT NOT NULL,
  input_key TEXT NOT NULL,
  output_hash TEXT NOT NULL,
  cursor_blob BLOB NOT NULL,
  generation INTEGER NOT NULL,
  PRIMARY KEY (mount_id, input_key, output_hash)
);
```

Barrier state can use the same idea:

```sql
CREATE TABLE barrier_buffer (
  pipe_hash INTEGER NOT NULL,
  instance_id INTEGER NOT NULL,
  generation INTEGER NOT NULL,
  depth INTEGER NOT NULL,
  row_idx INTEGER NOT NULL,
  cursor_blob BLOB NOT NULL,
  PRIMARY KEY (pipe_hash, instance_id, generation, depth, row_idx)
);
```

Current executable store tables:

```text
mounted_query_mount(mount_id, input_key, generation, sql)
mounted_query_dep(mount_id, input_key, dep_table)
mounted_query_output(mount_id, input_key, generation, cursor_id)
mounted_query_cursor(cursor_id, cursor_blob)
mounted_query_support(__support_cursor_id, support_table, support_row_id)
```

## Query Execution

Default execution should avoid storing full internal join working sets.

```text
runtime batch
-> temp input relation
-> SQLite query over input + fact tables
-> output cursor stream
-> durable diff against mounted_query_output
```

Persist the output boundary:

```text
mount_id
input_key
output_hash
cursor_blob
generation
```

Do not persist every internal join pair by default. Add persisted intermediate indexes later only for expensive queries that prove they need it.

## Invalidation

Dirty keys should be specific enough to wake affected mounted queries without scanning all mounts.

Minimum vocabulary:

```text
DirtyKey::Table(table)
DirtyKey::ColumnValue(table, column, value)
DirtyKey::File(file_id)
DirtyKey::Ref(ref_id)
DirtyKey::Row(row_id)
```

First implementation can be conservative:

```text
row inserted into table T
-> wake all mounts that depend on table T
```

Current executable behavior:

```text
FactStore::commit(gen, bus)
-> table dirty event for each changed relation
-> attached queue wakes parked rows by TABLE_DOMAIN/table_dirty_key(T)
-> SqlQueryComponent reruns the batch
-> mounted_query_output is replaced for mount_id + input_key
-> newly added output cursors continue downstream
-> removed supported output cursors retract supported rule rows
```

Later implementation:

```text
row inserted into table T with OP=getUser
-> wake mounts depending on T.OP=getUser
```

## Diff Semantics

Each mounted query rerun computes a new output set for the affected input key or batch.

```text
old = mounted_query_output rows for mount_id + input_key
new = rows emitted by rerun

additions = new - old
retractions = old - new
unchanged = old intersect new
```

Commit behavior:

```text
delete retracted output rows
insert added output rows
publish additions/retractions downstream
```

For LSP diagnostics, retraction means clearing diagnostics whose source output row disappeared.

For downstream rule rows, current retraction uses explicit support rows. Rule writes downstream of mounted SQL record which output cursor supported each written row. When that output disappears and no remaining support row points to the same fact row, the fact row is deleted.

## Recovery

On shutdown with SQLite backends:

```text
mounted_query
mounted_query_dep
mounted_query_output
barrier_buffer
dirty queue
```

must be enough to recover:

- what queries were mounted in the fact store
- which tables can wake them
- what outputs were last visible
- which parked rows were mid-pipe in the queue

On startup:

```text
load mounted_query rows
resume queued/parked rows from SqliteQueue
wake by table dirty keys as commits arrive
optionally rerun mounts marked dirty before shutdown
```

## Tests To Add

Locked tests:

```text
mounted_query_reacts_to_late_relation_write
mounted_query_rerun_emits_only_new_output_hashes
mounted_query_retraction_cascades_to_supported_rule_rows
sqlite_queue_revive_smoke
app_can_use_sqlite_queue_for_mounted_sql_parks
app_can_use_sqlite_fact_store_for_rule_rows
sprefa_run_accepts_sqlite_queue_db
sprefa_run_accepts_sqlite_fact_db
```

Remaining target tests:

```text
mounted_query_survives_full_sqlite_backend_reopen
store_backed_collect_buffers
```

Later slices:

```text
column-value dirty keys
row-level additions/retractions through downstream queues
durable dirty queue
store-backed collect buffers
query dependency extraction from SQL lowering for CTE-heavy SQL
restart integration for reopened fact store + queue + app state
```
