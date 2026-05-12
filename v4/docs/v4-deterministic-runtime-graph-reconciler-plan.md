# V4 Deterministic Runtime Graph Reconciler Plan

> For implementation in worktree `/private/tmp/sprefa-worktrees/codex-lsp-retraction-runtime` on branch `codex-lsp-retraction-runtime`.

## Goal

Build the Rust/SQLite substrate for deterministic retraction and resume:

- persisted runtime graph state
- URI-shaped identity interned through `_strings`
- source-coordinate values through `_where_bytes`
- support-based output reconciliation
- wake/event/subscription mechanics that can later host impure sources, stateful operators, polls, RSS, git diff, and editor/LSP events

No public sprf syntax for `combineLatest`, `merge`, `switchMap`, `poll`, `rss`, or `git.diff` in this slice. This is substrate and test harness only.

## Core Model

Use generic physical graph tables, but expose typed Rust APIs. Call sites must not insert arbitrary graph rows.

Physical storage nouns:

- `runtime_node`
- `runtime_edge`
- `runtime_value`
- `runtime_edge_value`
- `runtime_event`

Typed Rust nouns:

- `OwnerNode`
- `SourceNode`
- `OutputNode`
- `RowNode`
- `SubscribeEdge`
- `SupportEdge`
- `ActiveEdge`
- `RuntimeValue`

URIs are durable identities. Hashes are indexes/cache keys.

```text
owner_key = H(owner_uri)
value_key = H(canonical value payload)
row_key   = H(table_uri, canonical row payload)
```

Queue ids, `instance_id`, pointer addresses, insertion order, and monotonic process-local counters are scheduler details only. They must not define durable reconciler identity.

## Storage Shape

Runtime graph rows reference existing coord/interner tables:

- URI strings, kinds, modes, labels, and states are interned into `_strings`.
- Source-located runtime values point to `_where_bytes.id` through `value_ref_id`.
- Synthetic or cross-source payloads use `value_blob`.

Declare these through `FactStore<Cursor>` first. Do not add a custom SQL migration layer in this slice.

```text
runtime_node(
  node_uri_id,
  node_kind_id,
  ast_uri_id,
  parent_uri_id,
  input_key,
  source_hash,
  mode_id,
  generation
)

runtime_edge(
  edge_uri_id,
  edge_kind_id,
  from_uri_id,
  to_uri_id,
  label_id,
  generation
)

runtime_value(
  value_uri_id,
  value_key,
  value_kind_id,
  value_ref_id,
  value_blob,
  state_id,
  generation
)

runtime_edge_value(
  edge_uri_id,
  label_id,
  value_uri_id,
  generation
)

runtime_event(
  event_uri_id,
  source_uri_id,
  value_uri_id,
  generation,
  consumed
)
```

`runtime_edge_value` is edge-local state. This matters for cases like `combineLatest(table_a, table_a)`: both subscriptions point to the same source but need separate readiness slots.

`runtime_event` is append-only. Do not collapse events into source latest state. Latest source state can be added later as a cache, but replay/resume needs an event log.

## Typed API

Create `v4/src/runtime_graph.rs` and expose it from `v4/src/lib.rs`.

Minimum typed API:

```rust
pub struct RuntimeGraph {
    pub store: Arc<SprfStore>,
    pub facts: Arc<dyn FactStore<Cursor>>,
}

pub struct OwnerNode { pub uri_id: StringId, pub uri: Arc<str> }
pub struct SourceNode { pub uri_id: StringId, pub uri: Arc<str> }
pub struct OutputNode { pub uri_id: StringId, pub uri: Arc<str> }
pub struct RowNode { pub uri_id: StringId, pub uri: Arc<str>, pub row_key: Arc<str> }

pub struct SubscribeEdge { pub uri_id: StringId, pub uri: Arc<str> }
pub struct SupportEdge { pub uri_id: StringId, pub uri: Arc<str> }
pub struct ActiveEdge { pub uri_id: StringId, pub uri: Arc<str> }

pub struct RuntimeValue {
    pub uri_id: StringId,
    pub uri: Arc<str>,
    pub value_key: Arc<str>,
}

pub struct VisibleDelta {
    pub inserted: Vec<RowNode>,
    pub retracted: Vec<RowNode>,
}
```

Initial methods:

```rust
impl RuntimeGraph {
    pub fn new(store: Arc<SprfStore>, facts: Arc<dyn FactStore<Cursor>>) -> Self;

    pub fn declare_owner(
        &self,
        ast_uri: &str,
        parent_owner_uri: Option<&str>,
        input_key: &str,
        source_hash: &str,
        mode: &str,
        generation: u64,
    ) -> OwnerNode;

    pub fn declare_source(&self, source_uri: &str, generation: u64) -> SourceNode;
    pub fn declare_output_table(&self, table_uri: &str, generation: u64) -> OutputNode;

    pub fn declare_row(
        &self,
        table: &OutputNode,
        row_payload: &Cursor,
        generation: u64,
    ) -> RowNode;

    pub fn subscribe(
        &self,
        owner: &OwnerNode,
        label: &str,
        source: &SourceNode,
        generation: u64,
    ) -> SubscribeEdge;

    pub fn replace_active_child(
        &self,
        owner: &OwnerNode,
        label: &str,
        child: &OwnerNode,
        generation: u64,
    ) -> ActiveEdge;

    pub fn runtime_value_dirty(
        &self,
        source: &SourceNode,
        generation: u64,
    ) -> RuntimeValue;

    pub fn runtime_value_cursor_blob(
        &self,
        value_uri: &str,
        cursor: &Cursor,
        state: &str,
        generation: u64,
    ) -> RuntimeValue;

    pub fn runtime_value_where_bytes(
        &self,
        value_uri: &str,
        where_bytes: WhereBytesId,
        state: &str,
        generation: u64,
    ) -> RuntimeValue;

    pub fn record_edge_value(
        &self,
        edge: &SubscribeEdge,
        label: &str,
        value: &RuntimeValue,
        generation: u64,
    );

    pub fn append_source_event(
        &self,
        source: &SourceNode,
        value: &RuntimeValue,
        generation: u64,
    );

    pub fn replace_supports(
        &self,
        owner: &OwnerNode,
        table: &OutputNode,
        rows: &[Cursor],
        generation: u64,
    ) -> VisibleDelta;
}
```

The module owns validation:

```text
subscribe: owner -> source
support:   owner -> row
member_of: row   -> table
active:    owner -> owner
```

## Task 1: Identity Helpers And Table Declarations

Files:

- Create `v4/src/runtime_graph.rs`
- Modify `v4/src/lib.rs`
- Test `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add failing tests for stable URI/key construction:
   - same `ast_uri + parent_owner_uri + input_key` produces same owner URI/id/key
   - different input key produces different owner URI/id/key
   - kind/label/mode strings are present in `_strings`

2. Add `runtime_graph.rs` with:
   - constants for table names and columns
   - typed wrappers
   - `RuntimeGraph::new`
   - `declare_graph_tables`
   - URI constructors
   - string interning helper

3. Export module in `v4/src/lib.rs`.

4. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke identity_helpers_are_deterministic
```

Expected: pass.

Commit:

```bash
git add v4/src/runtime_graph.rs v4/src/lib.rs v4/tests/runtime_graph_smoke.rs
git commit -m "feat: add runtime graph identity substrate"
```

## Task 2: Graph Storage Helpers

Files:

- Modify `v4/src/runtime_graph.rs`
- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add failing tests:
   - owner/source/output/row nodes are written as `runtime_node` rows
   - subscribe/active/member edges are written as `runtime_edge` rows
   - `SqliteFactStore::open_file` persists graph rows across reopen

2. Implement:
   - `declare_owner`
   - `declare_source`
   - `declare_output_table`
   - `declare_row`
   - `subscribe`
   - `replace_active_child`

3. For row payloads:
   - compute `row_key` using the existing `FactStore::row_id_for(table, row)` shape when possible
   - create `row://<table-uri-id>/<row-key>` as row URI
   - insert `row --member_of--> table`

4. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke graph_rows_persist_through_sqlite_reopen
```

Expected: pass.

Commit:

```bash
git add v4/src/runtime_graph.rs v4/tests/runtime_graph_smoke.rs
git commit -m "feat: persist runtime graph nodes and edges"
```

## Task 3: Runtime Values And Events

Files:

- Modify `v4/src/runtime_graph.rs`
- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add failing tests:
   - dirty value stores no blob and uses `value_kind=dirty`
   - source-located value stores `value_ref_id` and no `value_blob`
   - synthetic cursor value stores `value_blob`
   - duplicate subscriptions to same source keep separate `runtime_edge_value` rows
   - appended events survive SQLite reopen and consumed events can be filtered

2. Implement:
   - `runtime_value_dirty`
   - `runtime_value_cursor_blob`
   - `runtime_value_where_bytes`
   - `record_edge_value`
   - `append_source_event`
   - read helpers used by tests:
     - `incoming_subscriptions(source)`
     - `edge_value(edge, label)`
     - `unconsumed_events(source)`
     - `mark_event_consumed(event_uri)`

3. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke runtime_values_and_events_are_durable
```

Expected: pass.

Commit:

```bash
git add v4/src/runtime_graph.rs v4/tests/runtime_graph_smoke.rs
git commit -m "feat: add runtime graph values and events"
```

## Task 4: Support Reconciliation

Files:

- Modify `v4/src/runtime_graph.rs`
- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add failing tests:
   - one owner retraction does not remove a row still supported by another owner
   - final support removal retracts the row
   - unchanged output set returns empty `VisibleDelta`
   - row membership edge remains stable for table membership

2. Implement `replace_supports`:
   - load existing `support` edges from owner to rows for table
   - build new row nodes from incoming row payloads
   - insert added support edges
   - delete removed support edges
   - only include a row in `retracted` when no remaining support edge points to it
   - include a row in `inserted` only when it was not visible before

3. Add deletion/hide mechanics using `FactStore::delete_matching` for graph support edges. Do not physically delete user fact rows in this task unless all support counts are zero and the row is graph-owned.

4. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke replace_supports_reconciles_visible_rows
```

Expected: pass.

Commit:

```bash
git add v4/src/runtime_graph.rs v4/tests/runtime_graph_smoke.rs
git commit -m "feat: reconcile runtime output supports"
```

## Task 5: Wake Dispatch Prototype

Files:

- Modify `v4/src/runtime_graph.rs`
- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add failing tests:
   - appending a dirty event to a source returns the owner subscribed to that source
   - appending a value event to a source returns the same owner
   - consumed events are not replayed
   - two subscribe edges to the same source are both found

2. Implement graph-level wake helpers:
   - `dispatch_wake(source, value, generation) -> Vec<OwnerNode>`
   - `dispatch_dirty(source, generation) -> Vec<OwnerNode>` only as a graph helper backed by dirty `RuntimeValue`, not as an EventBus compatibility shim
   - mark events consumed after returned owners are observed by the caller

3. Do not wire into `EventBus` yet. This is a graph substrate prototype.

4. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke wake_events_find_subscribed_owners
```

Expected: pass.

Commit:

```bash
git add v4/src/runtime_graph.rs v4/tests/runtime_graph_smoke.rs
git commit -m "feat: add runtime graph wake dispatch"
```

## Task 6: Deterministic Resume Harness

Files:

- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add a SQLite-backed test that:
   - creates graph state in a file DB
   - declares one owner
   - declares one source
   - subscribes owner to source
   - appends unconsumed wake event
   - drops all Rust graph objects
   - reopens the DB
   - loads unconsumed events
   - finds subscribed owner from persisted graph rows
   - marks event consumed
   - reopens again and proves it is not replayed

2. Assert no expected value depends on queue id, process-local counter, pointer address, or insertion order.

3. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke runtime_graph_resumes_from_sqlite
```

Expected: pass.

Commit:

```bash
git add v4/tests/runtime_graph_smoke.rs
git commit -m "test: prove runtime graph resumes from sqlite"
```

## Task 7: Test-Only Reactive Operator Harnesses

Files:

- Modify `v4/tests/runtime_graph_smoke.rs`

Steps:

1. Add test-only helpers in the test module:
   - `combine_latest_ready(graph, owner, labels) -> Option<Vec<RuntimeValue>>`
   - `merge_ready(graph, owner, changed_edge) -> Option<RuntimeValue>`
   - `switch_active_child(graph, owner, child) -> ActiveEdge`

2. Add tests:
   - `combineLatest(table_a, table_a)` has two subscribe edges and two independent readiness slots
   - `merge` emits when one changed edge is ready without waiting for other edges
   - `switchMap` replaces active child edge and persists the new child after SQLite reopen

3. Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke reactive_harnesses_use_durable_graph_state
```

Expected: pass.

Commit:

```bash
git add v4/tests/runtime_graph_smoke.rs
git commit -m "test: model reactive operators on runtime graph"
```

## Final Verification

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test runtime_graph_smoke
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target
cargo test --manifest-path v4/Cargo.toml --test sqlite_queue_revive_smoke
cargo test --manifest-path v4/Cargo.toml --test sprf_store_smoke
```

Expected: all pass.

Then:

```bash
git status --short
```

Expected: clean worktree after commits.

## Follow-Up Work Not In This Prototype

- Public sprf syntax for reactive operators.
- EventBus/QueueBackend full migration from `Wake::Key` to graph wake sources.
- Direct filesystem watcher integration.
- Custom SQL migration/index layer for graph tables.
- Source edit migration/unmount policy for changed `source_hash`.
