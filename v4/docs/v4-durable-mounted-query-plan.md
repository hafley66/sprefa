# V4 Durable Mounted Query Plan

## Scope

Foundation includes:

```text
effect_runtime
sprefa core
store
```

The next foundation slice is durable mounted query state. The runtime can now pause, wake, idle, and complete. Core and store need a durable layer that can survive shutdown, rerun affected query mounts after dirty events, and publish only output additions/retractions.

## Current State

Implemented:

- batched dispatch through `effect_runtime`
- parked rows and wake through `Yield` / `next`
- barrier lifecycle: `dispatch`, `idle`, `complete`
- `collect()` and `collect_ready(...)`
- rule declaration, writes, reads, predicates, and SQL batch-local query op
- fact table row identity through `_id`
- dirty row publish exists through `FactStore::commit`
- SQL query outputs are persisted to `mounted_query_output` through
  `FactStore` with `mount_id`, `input_key`, `generation`, `output_hash`,
  and `cursor_blob`

Current limitations:

- `CollectComponent` buffers in a component `Mutex`, not durable store state
- `SqlQueryComponent` caches by SQL, input batch, and referenced table row identities, but does not mount a live subscription
- `mounted_query_output` is append/dedup by full fact row today, not
  replacement by `(mount_id, input_key, output_hash)`
- dirty publish is row-oriented, not query-dependency-oriented
- stale outputs are not retracted or cleared after later writes

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

For downstream rule rows, retraction policy can start as replacement of the mounted output set. Full negative-diff propagation can come after the mount store exists.

## Recovery

On shutdown:

```text
mounted_query
mounted_query_dep
mounted_query_output
barrier_buffer
dirty queue
```

must be enough to recover:

- what queries were mounted
- which tables or keys can wake them
- what outputs were last visible
- which parked/barrier states were mid-generation if durable queue mode is active

On startup:

```text
load mounted_query rows
rebuild dirty dependency index
resume queued/parked rows
optionally rerun mounts marked dirty before shutdown
```

## Tests To Add

Red target tests:

```text
mounted_query_persists_outputs_by_mount_and_input_key
mounted_query_rerun_retracts_missing_row_after_late_insert
mounted_query_rerun_emits_only_new_output_hashes
mounted_query_survives_store_reopen
barrier_buffer_keys_by_scope_not_component_object
collect_does_not_mix_two_pipe_instances
```

First green slice:

```text
scope-keyed barrier state [done]
durable mounted_query_output table in store [first append-only slice done]
whole-output-set replacement per mount/input_key
table-level invalidation only
```

Later slices:

```text
column-value dirty keys
row-level additions/retractions through downstream queues
durable dirty queue
store-backed collect buffers
query dependency extraction from SQL lowering
```
