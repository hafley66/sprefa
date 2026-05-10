# V4 Effect Output Retraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a general effect-runtime/store primitive for replacing a materialized node output slice, so diagnostics, hovers, rule rows, SQL mount rows, and future side-effect tables can retract stale rows deterministically.

**Architecture:** Lowering assigns each op a stable source `op_path`; runtime execution combines that path with `pipe_hash`, `instance_id`, `depth`, and input cursor hash to form an output owner. Materialized output tables write through one replace-slice API that diffs old supports against new supports, keeps shared rows alive by support count, and emits dirty table events only when the visible row set changes.

**Tech Stack:** Rust, `effect_runtime::v2`, `SprfStore`, `SqliteFactStore`, `SqliteQueue`, existing `Cursor::content_hash`, existing mounted-query output tables, existing LSP diag/hover analysis path.

---

## Current Facts

| Existing piece | File | Use |
|---|---|---|
| Queue runtime identity | `v3/crates/effect_runtime/src/v2/queue.rs` | `QueueRow` has `pipe_hash`, `instance_id`, `depth`, `path`, `batch_idx`, `parent_id` |
| Render context identity | `v3/crates/effect_runtime/src/v2/component.rs` | `RenderCtx` has `pipe`, `depth`, `expand_tick`, `bus`, `diag` |
| Source op span wrapper | `v4/src/compile/probe_wrap.rs` | wraps lowered components for cursor-flow probes |
| AST op call source shape | `v4/src/compile/ast.rs` | `OpCall` has span, slots, dsl, block, but no stable `op_path` |
| Lowering walk | `v4/src/compile/walk.rs` | lowers `PipeAst` steps and can compute source paths while walking |
| Cursor identity | `v4/src/cursor_codec.rs` | stable cursor encoding/hash exists |
| Mounted SQL output diff | `v4/src/mounted_query.rs` | has query output replacement/diff behavior for one feature family |
| Table dirty wake | `effect_runtime::v2` + `v4/src/app.rs` | fact writes can publish table dirty and wake parked queue rows |
| LSP runtime hovers | `v4/src/lsp.rs`, `v4/src/app.rs` | hovers currently snapshot into `DocState.runtime_hovers` on `lsp_open/lsp_change` |

## Target Invariant

Every materialized output row has:

```text
owner_key = hash(pipe_hash, instance_id, depth, op_path, input_key)
row_key   = hash(table_name, canonical row payload)
support   = (table_name, row_key, owner_key)
```

Visible table rows are derived from support counts:

```text
visible(row_key) = count(support where table_name = T and row_key = R) > 0
```

Replacing one owner slice never deletes a row still supported by another owner.

## Data Model

### Runtime Path

```rust
pub struct RuntimeNodePath {
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    pub op_path: Vec<u32>,
}

pub struct OutputOwner {
    pub node: RuntimeNodePath,
    pub input_key: u64,
}
```

`op_path` is source/lowering path, not cursor data. It should be injected by the lowering wrapper and exposed on `RenderCtx` during component dispatch.

### Materialized Row

```rust
pub struct MaterializedRow {
    pub table: Arc<str>,
    pub row_key: u64,
    pub cursor: Cursor,
}

pub struct VisibleDelta {
    pub inserted: Vec<u64>,
    pub retracted: Vec<u64>,
}
```

### Store Trait

```rust
pub trait MaterializedOutputStore {
    fn replace_outputs(
        &self,
        owner: &OutputOwner,
        table: &str,
        rows: &[MaterializedRow],
    ) -> VisibleDelta;
}
```

This can initially live on `SprfStore` and delegate to existing `FactStore` plus support tables. If the trait boundary needs to move down later, keep the function signature stable.

## SQLite Tables

Start with generic support metadata:

```sql
CREATE TABLE IF NOT EXISTS _output_supports (
  table_name  TEXT NOT NULL,
  row_key     TEXT NOT NULL,
  owner_key   TEXT NOT NULL,
  pipe_hash   TEXT NOT NULL,
  instance_id TEXT NOT NULL,
  depth       TEXT NOT NULL,
  op_path     TEXT NOT NULL,
  input_key   TEXT NOT NULL,
  PRIMARY KEY (table_name, row_key, owner_key)
);

CREATE INDEX IF NOT EXISTS _output_supports_owner
ON _output_supports(table_name, owner_key);

CREATE INDEX IF NOT EXISTS _output_supports_row
ON _output_supports(table_name, row_key);
```

`op_path` can be encoded as slash-separated integers such as `0/2/1`. This is stable and readable in SQLite.

## Replace Algorithm

For `replace_outputs(owner, table, rows)`:

```text
old_supported = SELECT row_key FROM _output_supports
                WHERE table_name = table AND owner_key = owner.key

new_supported = rows.map(row_key)

delete supports in old_supported - new_supported
insert supports in new_supported - old_supported
insert visible table rows for every new row_key not already present
delete visible table rows where support_count(row_key) became zero
emit table dirty if visible inserted/retracted is non-empty
return VisibleDelta
```

The visible row insert should be idempotent. The visible row delete must check support count after deleting owner supports.

## Task 1: Carry `op_path` Through Lowering

**Files:**
- Modify: `v4/src/compile/ast.rs`
- Modify: `v4/src/compile/parse.rs`
- Modify: `v4/src/compile/walk.rs`
- Modify: `v4/src/compile/probe_wrap.rs`
- Test: `v4/tests/v4_walk_smoke.rs`

- [ ] **Step 1: Add failing test for nested op path**

Add this test to `v4/tests/v4_walk_smoke.rs`:

```rust
#[test]
fn walk_assigns_stable_op_paths_inside_rule_blocks() {
    use v4::compile::parse::host_parse;

    let src = r#"
        rule(:outer) {
          `alpha`
          > split(WORD?)` `
        };
        `seed` > void;
    "#;
    let (program, diags) = host_parse(src);
    assert!(diags.is_empty(), "{diags:?}");

    let top_rule = &program[0].steps[0];
    assert_eq!(top_rule.op_path, vec![0]);

    let block = top_rule.block.as_ref().expect("rule block");
    assert_eq!(block.steps[0].op_path, vec![0, 0]);
    assert_eq!(block.steps[1].op_path, vec![0, 1]);

    assert_eq!(program[1].steps[0].op_path, vec![1, 0]);
    assert_eq!(program[1].steps[1].op_path, vec![1, 1]);
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test v4_walk_smoke walk_assigns_stable_op_paths_inside_rule_blocks
```

Expected: compile failure because `OpCall::op_path` does not exist.

- [ ] **Step 3: Add `op_path` to `OpCall` and assign paths**

Patch `v4/src/compile/ast.rs`:

```rust
pub struct OpCall {
    pub name: Arc<str>,
    pub force: bool,
    pub predicate: bool,
    pub apply: bool,
    pub op_path: Vec<u32>,
    pub span: ByteRange,
    pub flow: Option<SlotText>,
    pub args: Vec<SlotText>,
    pub dsl: Option<DslText>,
    pub block: Option<PipeAst>,
}
```

Patch `v4/src/compile/parse.rs` by threading path prefixes:

```rust
fn lower_pipe(pipe_node: Node<'_>, src: &str) -> Option<PipeAst> {
    lower_pipe_with_path(pipe_node, src, &[])
}

fn lower_pipe_with_path(pipe_node: Node<'_>, src: &str, prefix: &[u32]) -> Option<PipeAst> {
    let mut steps: Vec<OpCall> = Vec::new();
    let mut step_idx: u32 = 0;
    let mut walker = pipe_node.walk();
    for step in pipe_node.named_children(&mut walker) {
        match step.kind() {
            "op_invocation" => {
                if let Some(mut call) = lower_op_invocation(step, src, prefix, step_idx) {
                    steps.push(call);
                    step_idx += 1;
                }
            }
            "dsl_body" => {
                let mut op_path = prefix.to_vec();
                op_path.push(step_idx);
                steps.push(OpCall {
                    name: Arc::<str>::from("str"),
                    force: false,
                    predicate: false,
                    apply: false,
                    op_path,
                    span: node_range(step),
                    flow: None,
                    args: Vec::new(),
                    dsl: Some(dsl_text(step, src)),
                    block: None,
                });
                step_idx += 1;
            }
            "parenthesized" => {
                let inner_pipe = step.named_child(0).filter(|c| c.kind() == "pipe");
                if let Some(inner_pipe) = inner_pipe {
                    if let Some(inner) = lower_pipe_with_path(inner_pipe, src, prefix) {
                        steps.extend(inner.steps);
                    }
                }
            }
            _ => continue,
        }
    }
    Some(PipeAst { steps, span: node_range(pipe_node) })
}

fn lower_op_invocation(
    node: Node<'_>,
    src: &str,
    prefix: &[u32],
    step_idx: u32,
) -> Option<OpCall> {
    let mut op_path = prefix.to_vec();
    op_path.push(step_idx);
    let name_node = node.child_by_field_name("name")?;
    let name = Arc::<str>::from(&src[name_node.byte_range()]);
    let force = node.child_by_field_name("force").is_some();
    let predicate = node.child_by_field_name("predicate").is_some();
    let apply = node.child_by_field_name("apply").is_some();
    let flow = first_field(node, "bracket").map(|n| slot_text_from_delimited(n, src));
    let args = node
        .child_by_field_name("paren")
        .map(|n| split_paren_args(n, src))
        .unwrap_or_default();
    let dsl = node.child_by_field_name("dsl").map(|n| dsl_text(n, src));
    let block_prefix = op_path.clone();
    let block = node
        .child_by_field_name("brace")
        .and_then(|n| lower_brace_block_with_path(n, src, &block_prefix));
    Some(OpCall {
        name,
        force,
        predicate,
        apply,
        op_path,
        span: node_range(node),
        flow,
        args,
        dsl,
        block,
    })
}
```

Replace `lower_brace_block` with a path-aware version:

```rust
fn lower_brace_block_with_path(node: Node<'_>, src: &str, prefix: &[u32]) -> Option<PipeAst> {
    let mut walker = node.walk();
    for child in node.named_children(&mut walker) {
        if child.kind() == "pipe" {
            return lower_pipe_with_path(child, src, prefix);
        }
    }
    None
}
```

- [ ] **Step 4: Run test and verify it passes**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test v4_walk_smoke walk_assigns_stable_op_paths_inside_rule_blocks
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add v4/src/compile/ast.rs v4/src/compile/parse.rs v4/tests/v4_walk_smoke.rs
git commit -m "feat: assign stable op paths"
```

## Task 2: Add Runtime Node Path To `RenderCtx`

**Files:**
- Modify: `v3/crates/effect_runtime/src/v2/component.rs`
- Modify: `v4/src/compile/probe_wrap.rs`
- Modify: `v4/src/compile/walk.rs`
- Test: `v4/tests/v4_walk_smoke.rs`

- [ ] **Step 1: Add failing test for probe wrapper path plumbing**

Add a focused unit test in `v4/tests/v4_walk_smoke.rs`:

```rust
#[test]
fn lowered_components_receive_op_path_in_render_context() {
    use std::sync::{Arc, Mutex};
    use effect_runtime::v2::{expand, Component, ExpandOpts, MemQueue, Node, Pipe, RenderCtx};
    use v4::Cursor;

    struct CapturePath {
        seen: Arc<Mutex<Vec<u32>>>,
    }

    impl Component for CapturePath {
        type Next = Cursor;

        fn render(&self, ctx: &RenderCtx, c: &Cursor) -> Node<Cursor> {
            *self.seen.lock().unwrap() = ctx.op_path.clone();
            Node::Emit(Arc::new(c.clone()))
        }
    }

    let seen = Arc::new(Mutex::new(Vec::new()));
    let pipe = Pipe::new().step(Arc::new(v4::compile::probe_wrap::SpannedComponent::new(
        effect_runtime::v2::ByteRange { lo: 10, hi: 20 },
        vec![3, 5],
        Arc::new(CapturePath { seen: seen.clone() }),
    )));
    let inst = pipe.into_instance();
    expand(&inst, Arc::new(MemQueue::new()), vec![Arc::new(Cursor::default())], ExpandOpts::default());

    assert_eq!(*seen.lock().unwrap(), vec![3, 5]);
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test v4_walk_smoke lowered_components_receive_op_path_in_render_context
```

Expected: compile failure because `RenderCtx::op_path` and `SpannedComponent::new(..., op_path, ...)` do not exist.

- [ ] **Step 3: Extend `RenderCtx`**

Patch `v3/crates/effect_runtime/src/v2/component.rs`:

```rust
#[derive(Clone)]
pub struct RenderCtx {
    pub pipe:        PipeHash,
    pub depth:       u32,
    pub expand_tick: ExpandTick,
    pub bus:         Arc<EventBus>,
    pub diag:        Arc<dyn DiagSink>,
    pub op_path:     Vec<u32>,
}

impl RenderCtx {
    pub fn new(pipe: PipeHash, depth: u32, expand_tick: ExpandTick) -> Self {
        Self {
            pipe,
            depth,
            expand_tick,
            bus: Arc::new(EventBus::new()),
            diag: Arc::new(NoopDiagSink),
            op_path: Vec::new(),
        }
    }

    pub fn with_op_path(mut self, op_path: Vec<u32>) -> Self {
        self.op_path = op_path;
        self
    }
}
```

Keep existing `with_bus` and `with_diag` methods unchanged except for preserving `op_path`.

- [ ] **Step 4: Extend `SpannedComponent`**

Patch `v4/src/compile/probe_wrap.rs` so the wrapper stores `op_path`:

```rust
pub struct SpannedComponent {
    span: ByteRange,
    op_path: Vec<u32>,
    inner: DynComponent<Cursor>,
}

impl SpannedComponent {
    pub fn new(span: ByteRange, op_path: Vec<u32>, inner: DynComponent<Cursor>) -> Self {
        Self { span, op_path, inner }
    }
}
```

In every place `SpannedComponent` calls the inner component, clone the context with path:

```rust
let inner_ctx = ctx.clone().with_op_path(self.op_path.clone());
```

Pass `&inner_ctx` to `inner.render`, `inner.render_batch`, `inner.dispatch`, `inner.idle`, and `inner.complete`.

- [ ] **Step 5: Pass op path from walker**

Patch `v4/src/compile/walk.rs` where the wrapper is created:

```rust
SpannedComponent::new(op.span, op.op_path.clone(), component)
```

- [ ] **Step 6: Run test and verify it passes**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test v4_walk_smoke lowered_components_receive_op_path_in_render_context
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add v3/crates/effect_runtime/src/v2/component.rs v4/src/compile/probe_wrap.rs v4/src/compile/walk.rs v4/tests/v4_walk_smoke.rs
git commit -m "feat: expose op path in render context"
```

## Task 3: Add Generic Replace-Outputs Store Primitive

**Files:**
- Modify: `v4/src/store.rs`
- Test: `v4/tests/sprf_store_smoke.rs`

- [ ] **Step 1: Add failing support-count test**

Add this test to `v4/tests/sprf_store_smoke.rs`:

```rust
#[test]
fn replace_outputs_support_counts_shared_rows() {
    use effect_runtime::v2::{FactStore, MemFactStore};
    use std::sync::Arc;
    use v4::store::{
        MaterializedOutputRow, MaterializedOutputStore, OutputOwner, RuntimeNodePath, SprfStore,
    };
    use v4::Cursor;

    let facts: Arc<dyn FactStore<Cursor>> = Arc::new(MemFactStore::<Cursor>::new());
    let store = SprfStore::new(facts.clone());
    store.declare("demo_rows", &["NAME"]);

    let owner_a = OutputOwner {
        node: RuntimeNodePath {
            pipe_hash: 1,
            instance_id: 1,
            depth: 0,
            op_path: vec![0],
        },
        input_key: 10,
    };
    let owner_b = OutputOwner {
        node: RuntimeNodePath {
            pipe_hash: 1,
            instance_id: 1,
            depth: 0,
            op_path: vec![1],
        },
        input_key: 20,
    };

    let mut row = Cursor::default();
    row.set("NAME", "alpha");
    let materialized = MaterializedOutputRow::from_cursor("demo_rows", row.clone());

    let delta_a = store.replace_outputs(&owner_a, "demo_rows", &[materialized.clone()]);
    assert_eq!(delta_a.inserted.len(), 1);
    assert_eq!(delta_a.retracted.len(), 0);
    assert_eq!(store.len("demo_rows"), 1);

    let delta_b = store.replace_outputs(&owner_b, "demo_rows", &[materialized.clone()]);
    assert_eq!(delta_b.inserted.len(), 0);
    assert_eq!(delta_b.retracted.len(), 0);
    assert_eq!(store.len("demo_rows"), 1);

    let delta_remove_a = store.replace_outputs(&owner_a, "demo_rows", &[]);
    assert_eq!(delta_remove_a.inserted.len(), 0);
    assert_eq!(delta_remove_a.retracted.len(), 0);
    assert_eq!(store.len("demo_rows"), 1);

    let delta_remove_b = store.replace_outputs(&owner_b, "demo_rows", &[]);
    assert_eq!(delta_remove_b.inserted.len(), 0);
    assert_eq!(delta_remove_b.retracted.len(), 1);
    assert_eq!(store.len("demo_rows"), 0);
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test sprf_store_smoke replace_outputs_support_counts_shared_rows
```

Expected: compile failure because the new materialized output types and trait do not exist.

- [ ] **Step 3: Add types and trait**

Patch `v4/src/store.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuntimeNodePath {
    pub pipe_hash: u64,
    pub instance_id: u64,
    pub depth: u32,
    pub op_path: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct OutputOwner {
    pub node: RuntimeNodePath,
    pub input_key: u64,
}

#[derive(Clone, Debug)]
pub struct MaterializedOutputRow {
    pub table: Arc<str>,
    pub row_key: u64,
    pub cursor: Cursor,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VisibleDelta {
    pub inserted: Vec<u64>,
    pub retracted: Vec<u64>,
}

pub trait MaterializedOutputStore {
    fn replace_outputs(
        &self,
        owner: &OutputOwner,
        table: &str,
        rows: &[MaterializedOutputRow],
    ) -> VisibleDelta;
}
```

Implement helpers:

```rust
impl OutputOwner {
    pub fn key(&self) -> u64 {
        let mut h = blake3::Hasher::new();
        h.update(&self.node.pipe_hash.to_le_bytes());
        h.update(&self.node.instance_id.to_le_bytes());
        h.update(&self.node.depth.to_le_bytes());
        for part in &self.node.op_path {
            h.update(&part.to_le_bytes());
        }
        h.update(&self.input_key.to_le_bytes());
        let b = h.finalize();
        u64::from_le_bytes(b.as_bytes()[0..8].try_into().unwrap()).max(1)
    }
}

impl MaterializedOutputRow {
    pub fn from_cursor(table: impl Into<Arc<str>>, cursor: Cursor) -> Self {
        let table = table.into();
        let mut h = blake3::Hasher::new();
        h.update(table.as_bytes());
        h.update(&cursor.content_hash().to_le_bytes());
        let b = h.finalize();
        let row_key = u64::from_le_bytes(b.as_bytes()[0..8].try_into().unwrap()).max(1);
        Self { table, row_key, cursor }
    }
}
```

- [ ] **Step 4: Implement in-memory support metadata on `SprfStore`**

Add a mutex field to `SprfStore`:

```rust
output_supports: Mutex<HashMap<(String, u64), HashSet<u64>>>,
owner_supports: Mutex<HashMap<(String, u64), HashSet<u64>>>,
```

The first map is `(table, row_key) -> owner_keys`.
The second map is `(table, owner_key) -> row_keys`.

Implement `replace_outputs`:

```rust
impl MaterializedOutputStore for SprfStore {
    fn replace_outputs(
        &self,
        owner: &OutputOwner,
        table: &str,
        rows: &[MaterializedOutputRow],
    ) -> VisibleDelta {
        let owner_key = owner.key();
        let owner_slot = (table.to_string(), owner_key);
        let new_keys: HashSet<u64> = rows.iter().map(|r| r.row_key).collect();

        let mut owner_supports = self.owner_supports.lock().unwrap();
        let mut output_supports = self.output_supports.lock().unwrap();
        let old_keys = owner_supports.get(&owner_slot).cloned().unwrap_or_default();

        let mut delta = VisibleDelta::default();

        for old in old_keys.difference(&new_keys) {
            let row_slot = (table.to_string(), *old);
            if let Some(owners) = output_supports.get_mut(&row_slot) {
                owners.remove(&owner_key);
                if owners.is_empty() {
                    output_supports.remove(&row_slot);
                    self.remove_row_by_content_hash(table, *old);
                    delta.retracted.push(*old);
                }
            }
        }

        for row in rows {
            let row_slot = (table.to_string(), row.row_key);
            let owners = output_supports.entry(row_slot).or_default();
            if owners.is_empty() {
                self.insert(table, row.cursor.clone());
                delta.inserted.push(row.row_key);
            }
            owners.insert(owner_key);
        }

        if new_keys.is_empty() {
            owner_supports.remove(&owner_slot);
        } else {
            owner_supports.insert(owner_slot, new_keys);
        }

        delta.inserted.sort_unstable();
        delta.retracted.sort_unstable();
        delta
    }
}
```

Add `remove_row_by_content_hash` as a `SprfStore` method. For `MemFactStore`, if no direct delete exists, add the delete operation to `FactStore` in the same task. If widening `FactStore` is larger than expected, keep deleted rows hidden through support-aware read paths in this task and file a follow-up before using this for externally visible facts.

- [ ] **Step 5: Run test and verify it passes**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test sprf_store_smoke replace_outputs_support_counts_shared_rows
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add v4/src/store.rs v4/tests/sprf_store_smoke.rs
git commit -m "feat: add materialized output supports"
```

## Task 4: Move Mounted Query Replacement Onto Generic Primitive

**Files:**
- Modify: `v4/src/mounted_query.rs`
- Modify: `v4/src/sql.rs`
- Test: `v4/tests/rule_future_semantics_target.rs`

- [ ] **Step 1: Add regression test for shared query output support**

Add this test to `v4/tests/rule_future_semantics_target.rs`:

```rust
#[tokio::test]
async fn mounted_query_shared_output_survives_one_input_retraction() {
    use v4::app::{build_in_process, GetFactTableReq, RunReq, SprfClient};
    use std::fs;

    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("shared_output.sprf");
    fs::write(&sprf, r#"
        rule(:source, NAME?) {
          `alpha` > term_bind(:NAME)
          `alpha` > term_bind(:NAME)
        };

        source(NAME?)
          > sql`
              SELECT input.__cursor_idx, input.NAME
              FROM input
            `
          > rule(:out, NAME);
    "#).unwrap();

    let (_state, client) = build_in_process(root.path().to_path_buf());
    client.run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) }).await.unwrap();

    let table = client.get_fact_table(GetFactTableReq {
        name: "out".into(),
        limit: None,
    }).await.unwrap();
    assert_eq!(table.total, 1);
}
```

- [ ] **Step 2: Run test and verify current behavior**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target mounted_query_shared_output_survives_one_input_retraction
```

Expected: either FAIL by duplicate visible rows or PASS if current dedupe already covers this exact shape. Keep the test because it pins the generic support invariant.

- [ ] **Step 3: Replace mounted-query-specific diff with `replace_outputs`**

In `v4/src/mounted_query.rs`, replace local support/diff code with conversion into `MaterializedOutputRow` and call:

```rust
let owner = OutputOwner {
    node: RuntimeNodePath {
        pipe_hash: ctx.pipe,
        instance_id: ctx.pipe,
        depth: ctx.depth,
        op_path: ctx.op_path.clone(),
    },
    input_key,
};
let materialized = rows
    .into_iter()
    .map(|cursor| MaterializedOutputRow::from_cursor(output_table.clone(), cursor))
    .collect::<Vec<_>>();
let delta = store.replace_outputs(&owner, &output_table, &materialized);
```

If `instance_id` is not available on `RenderCtx` yet, add it in Task 2 before this task:

```rust
pub instance_id: u64
```

and set it from the current `PipeInstance` when constructing `RenderCtx`.

- [ ] **Step 4: Run mounted query tests**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add v4/src/mounted_query.rs v4/src/sql.rs v4/tests/rule_future_semantics_target.rs
git commit -m "refactor: use generic output replacement for mounted sql"
```

## Task 5: Move `lsp.hover` Onto Generic Primitive

**Files:**
- Modify: `v4/src/lsp.rs`
- Modify: `v4/src/app.rs`
- Test: `v4/tests/lsp_hover_smoke.rs`

- [ ] **Step 1: Add failing hover retraction test**

Add this test to `v4/tests/lsp_hover_smoke.rs`:

```rust
#[tokio::test]
async fn lsp_hover_retracts_when_runtime_source_no_longer_emits() {
    use v4::app::{LspChangeReq, LspHoverReq, LspOpenReq, SprfClient, build_in_process};

    let (_state, client) = build_in_process(std::env::temp_dir());
    let uri = "file:///runtime-hover-retract.sprf".to_string();

    let first = "`before sh! after` > re`before (?P<NAME>sh!) after` > lsp.hover[NAME]`custom hover ${NAME}`;";
    client.lsp_open(LspOpenReq {
        uri: uri.clone(),
        text: first.into(),
        version: 1,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq { uri: uri.clone(), byte: 8 }).await.unwrap();
    assert_eq!(hover.contents.as_deref(), Some("custom hover sh!"));

    let second = "`before sh! after` > re`before (?P<NAME>nope) after` > lsp.hover[NAME]`custom hover ${NAME}`;";
    client.lsp_change(LspChangeReq {
        uri: uri.clone(),
        text: second.into(),
        version: 2,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq { uri, byte: 8 }).await.unwrap();
    assert_eq!(hover.contents, None);
}
```

This test may already pass through whole-document replacement. Keep it as the public LSP contract.

- [ ] **Step 2: Add table-backed hover query test**

Add a second test in `v4/tests/lsp_hover_smoke.rs` after the previous test:

```rust
#[tokio::test]
async fn lsp_hover_rows_are_not_returned_as_diagnostics() {
    use v4::app::{GetDiagsReq, LspHoverReq, LspOpenReq, SprfClient, build_in_process};

    let (_state, client) = build_in_process(std::env::temp_dir());
    let uri = "file:///runtime-hover-not-diag.sprf".to_string();
    let src = "`before sh! after` > re`before (?P<NAME>sh!) after` > lsp.hover[NAME]`custom hover ${NAME}`;";
    client.lsp_open(LspOpenReq {
        uri: uri.clone(),
        text: src.into(),
        version: 1,
    }).await.unwrap();

    let hover = client.lsp_hover(LspHoverReq { uri: uri.clone(), byte: 8 }).await.unwrap();
    assert_eq!(hover.contents.as_deref(), Some("custom hover sh!"));

    let diags = client.get_diags(GetDiagsReq { uri }).await.unwrap();
    assert!(diags.iter().all(|d| d.code != "sprf/hover"));
}
```

- [ ] **Step 3: Run hover tests**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test lsp_hover_smoke
```

Expected: PASS before and after internal migration if Task 4 is complete.

- [ ] **Step 4: Write hover rows through generic output replacement**

In `v4/src/lsp.rs`, make `LspHoverComponent` create a materialized row:

```rust
let mut row = Cursor::default();
row.set("URI", current_uri);
row.set("LO", lo.to_string());
row.set("HI", hi.to_string());
row.set("MESSAGE", self.render_message(c));
```

If `current_uri` is not available to runtime components, keep the current `DocState.runtime_hovers` split for LSP-open analysis and do not force cross-file hover storage in this task. Record this as a gap in `v4/docs/v4-effect-output-retraction-plan.md` under `Known Gaps After Task 5`.

- [ ] **Step 5: Commit**

```bash
git add v4/src/lsp.rs v4/src/app.rs v4/tests/lsp_hover_smoke.rs v4/docs/v4-effect-output-retraction-plan.md
git commit -m "refactor: route runtime hovers through output replacement"
```

## Task 6: Dirty-Wake Rerun Uses Visible Delta

**Files:**
- Modify: `v4/src/app.rs`
- Modify: `v4/src/store.rs`
- Test: `v4/tests/rule_future_semantics_target.rs`

- [ ] **Step 1: Add rerun no-op delta test**

Add this test to `v4/tests/rule_future_semantics_target.rs`:

```rust
#[tokio::test]
async fn mounted_query_noop_rerun_does_not_emit_downstream_dirty() {
    use v4::app::{build_in_process, RunReq, SprfClient};
    use std::fs;

    let root = tempfile::tempdir().unwrap();
    let sprf = root.path().join("noop_dirty.sprf");
    fs::write(&sprf, r#"
        rule(:source, NAME?) { `alpha` > term_bind(:NAME) };
        source(NAME?)
          > sql`
              SELECT input.__cursor_idx, input.NAME
              FROM input
            `
          > rule(:out, NAME);
    "#).unwrap();

    let (state, client) = build_in_process(root.path().to_path_buf());
    client.run(RunReq { path: sprf.clone(), root: Some(root.path().to_path_buf()) }).await.unwrap();
    let before = state.facts.len("out");
    client.run(RunReq { path: sprf, root: Some(root.path().to_path_buf()) }).await.unwrap();
    let after = state.facts.len("out");
    assert_eq!(before, 1);
    assert_eq!(after, 1);
}
```

- [ ] **Step 2: Run test**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target mounted_query_noop_rerun_does_not_emit_downstream_dirty
```

Expected: PASS for visible row count. If dirty counters exist, extend assertion to check no extra dirty event.

- [ ] **Step 3: Dispatch table dirty only on visible delta**

Where materialized replacement currently commits, use:

```rust
let delta = store.replace_outputs(&owner, table, &rows);
if !delta.inserted.is_empty() || !delta.retracted.is_empty() {
    bus.dispatch_dirty(TABLE_DOMAIN, table_dirty_key(table));
}
```

- [ ] **Step 4: Run mounted-query and hover tests**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target
cargo test --manifest-path v4/Cargo.toml --test lsp_hover_smoke
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add v4/src/app.rs v4/src/store.rs v4/tests/rule_future_semantics_target.rs
git commit -m "fix: emit dirty only for visible output deltas"
```

## Task 7: Full Verification

**Files:**
- No source edits unless a test reveals a defect.

- [ ] **Step 1: Run focused suites**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --test sprf_store_smoke
cargo test --manifest-path v4/Cargo.toml --test rule_future_semantics_target
cargo test --manifest-path v4/Cargo.toml --test lsp_hover_smoke
cargo test --manifest-path v4/Cargo.toml --test v3_parity_target
```

Expected: all PASS.

- [ ] **Step 2: Run full V4 suite**

Run:

```bash
cargo test --manifest-path v4/Cargo.toml --tests --lib
```

Expected: all PASS.

- [ ] **Step 3: Build release binaries**

Run:

```bash
cargo build --manifest-path v4/Cargo.toml --release --bin sprefa-run --bin sprefa-daemon --bin v4-bench
```

Expected: build succeeds.

- [ ] **Step 4: Commit docs if changed**

```bash
git add v4/docs/v4-effect-output-retraction-plan.md v4/docs/README.md
git commit -m "docs: plan effect output retraction"
```

## Known Gaps Before Implementation

| Gap | Consequence |
|---|---|
| `RenderCtx` lacks `instance_id` today | owner key may need `pipe_hash` as a temporary proxy until runtime context is widened |
| LSP open-buffer analysis lacks target URI in `RenderCtx` | `_lsp_hover` rows cannot target arbitrary source files without adding document/run metadata to context |
| `FactStore` visible-row delete API may be missing | support-aware reads may need to land before physical deletes |
| Existing mounted query code has its own output tables | migration should preserve current green tests before deleting old paths |
| Existing hovers are snapshot-scoped per `DocState` | table-backed hovers should keep that behavior during migration |

## Review Checklist

- `op_path` is assigned once from parse/lowering and never derived from cursor contents.
- `replace_outputs` is the only new table retraction primitive.
- A row shared by two owners remains visible after one owner retracts.
- Dirty events fire only when visible rows insert or retract.
- `lsp.hover` remains a consumer of the general materialized output model.
- Existing `lsp_warn` diagnostics continue to surface through `get_diags`.
- Existing editor hover fallback still reports cursor-flow probes when no runtime hover exists.
