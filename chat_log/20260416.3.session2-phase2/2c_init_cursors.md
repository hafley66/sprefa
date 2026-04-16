# 2c — init_cursors

Stand up `v2/src/_7_init_cursors.rs`. Collapse `_7_runner.rs` callers
into it. The function is the single evaluator entry point.

## Prereqs

2a + 2b (`SqliteStore` impls `Store`). 2c produces the stream that 2e
(DocSession / bin rewire) consumes.

## Scope

```
v2/src/_7_init_cursors.rs        NEW  (~220 LOC)
v2/src/lib.rs                    pub mod _7_init_cursors; remove _7_runner;
v2/src/_7_runner.rs              DELETE
```

## Files

### v2/src/_7_init_cursors.rs

Clone Z3 section `### src/_7_init_cursors.rs` (lines 1238–1381 of
design doc). Four public items:

- `struct InitInputs` — full plumbing bundle (exprs, config, store,
  reader, writer, registry, mutations_tx, cancel, run_id, scanner_hash)
- `fn init_cursors(inp: InitInputs) -> BoxStream<'static, RunEvent>`
- `struct RunReport` — `cursors_by_expr: HashMap<Arc<str>, Vec<Cursor>>`,
  `diags: Vec<Box<dyn Diagnostic>>`
- `async fn collect_run_report(s: BoxStream<'static, RunEvent>) -> RunReport`
  (Z3 lines 1448–1461)

And five private helpers:

- `build_op_ctx(inp, expr, rs) -> OpCtx` (Z3 lines 1316–1332)
- `topo_sort_exprs(exprs) -> Vec<&CursorExpr>` (Z3 lines 1333–1337)
- `build_expr_table_spec(expr) -> ExprTableSpec` (Z3 lines 1339–1352)
- `build_batch_from_result_store(rs, exprs, scanner_hash) -> Batch` (Z3
  lines 1354–1367)
- `row_insert_from_captures(cm) -> RowInsert` (Z3 lines 1369–1380)

### v2/src/lib.rs

Replace `pub mod _7_runner;` with `pub mod _7_init_cursors;`. Remove
any re-export of `_7_runner` types.

### v2/src/_7_runner.rs

Delete. Any remaining callers must be rewired in 2e.

### Callsite inventory (for 2e reference)

```
rg "use (crate|v2)::_7_runner"
rg "_7_runner::"
```

Expect hits in: `analysis.rs`, `bin/sprefa_v2.rs`, maybe a test or two.
Each becomes an `init_cursors(InitInputs { ... })` call in 2e.

## Z3 deviations

- **DiagSink in build_op_ctx**: Z3 uses `DiagSink(Arc::new(|_d| {
  /* collected at a higher layer */ }))`. That drops diagnostics. Land
  it with a real `Arc<Mutex<Vec<Box<dyn Diagnostic>>>>` append sink
  owned by `init_cursors`'s stream body, so diagnostics the ops emit
  flow out as `RunEvent::Diag` alongside cursors. Same pattern as the
  existing `_7_runner.rs` sink.

- **topo_sort_exprs fallback**: when cycle detected, emit a
  `RunEvent::Diag` with a `CycleDiagnostic`, then run the exprs in
  declaration order. Do not panic. Cycle-diag code = `"init/xref-cycle"`.

- **build_expr_table_spec → schema_hash_of / extract_hash_of** is a
  chicken-and-egg in Z3 lines 1345. Land it as:
  ```rust
  let captures = ...;
  let schema_hash_stub = Arc::from("");
  let extract_hash = crate::store::_3_ddl::extract_hash_of(expr);
  let mut spec = ExprTableSpec { expr_name, namespace, captures, schema_hash: schema_hash_stub, extract_hash };
  spec.schema_hash = crate::store::_3_ddl::schema_hash_of(&spec);
  spec
  ```

## Tests

`v2/tests/init_cursors.rs` (new):

1. `empty_exprs_yields_only_done` — `InitInputs` with `exprs = vec![]`
   → stream yields exactly `RunEvent::Done`.
2. `single_anonymous_expr_yields_cursors` — no rule() wrapper, just a
   bare pipeline → emits `Cursor { expr_name: None, .. }` then
   `ExprDone { expr_name: None }`.
3. `named_expr_flushes_to_store` — `rule(R) > ...` → after `Done`,
   `store.query_expr("R", default_where()).await?` returns the
   expected rows.
4. `xref_topo_order_respected` — two rules where B references A via
   `${A.$V}` declared in reverse → A runs first; B sees A's captures.
5. `cancel_mid_stream_yields_done_and_aborts` — spawn init_cursors,
   cancel after 1 Cursor event, expect ≤ 10ms to reach `Done` and zero
   leaked tasks.

Fixtures use `MemReader` + `SqliteStore::open_memory` +
`mpsc::channel(4)` with `AutoApprove` handler.

## Verify

```
cd v2 && cargo test -p v2 --test init_cursors
cd v2 && cargo build --tests                   # picks up the _7_runner deletion
```

Expect the first pass to fail at existing callsites (`analysis.rs`,
`bin/*`). Those are rewired in 2e, so the Phase-2 green bar is staged:

- After 2c: `cargo test --test init_cursors` green; other tests /
  binaries error on missing `_7_runner`. Acceptable.
- After 2e: all green.

## Exit state

- `_7_init_cursors.rs` compiles
- 5 integration tests pass
- `_7_runner.rs` deleted; callsites visibly broken for 2e to fix
- Stream semantics (topo, cancel, named vs anonymous, flush timing) verified
