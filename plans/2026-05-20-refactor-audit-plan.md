# Refactor audit plan — 2026-05-20

Consolidated findings from three parallel Plan agents over the v4 runtime/graph
store layer, the v4 compile/lower layer, and the v3 effects lib + v4 bridge.
Goal: turn the audit into a sequenced refactor with concrete edit targets.

Audience: anyone touching `v4/src/compile/**`, `v4/src/*runtime*`, `v4/src/app.rs`,
`v4/src/v2_ops.rs`, or `v3/crates/effect_runtime/src/v2/**`.

Pre-reads:
- `plans/2026-05-19-v4-worst-audit.md` for prior worst-case audit context.
- `plans/2026-05-20-instance-leak-and-memory-control.md` for the in-flight
  fact-store work this refactor must not collide with.

---

## 0. Audit origin and gaps

Three agents, one report each. Scopes were disjoint by file list:

| agent | scope | report state |
|---|---|---|
| graph/store | runtime_graph.rs, store.rs, mounted_query.rs, memo*.rs, fact.rs, source*.rs, dirty_source.rs, cursor_codec.rs, fixpoint.rs | top-findings block truncated at head; open questions + "fine" list received |
| compile/lower | compile/**, rule.rs, stratify.rs, term.rs, template.rs, sql.rs, v2_ops.rs | full |
| effects lib + bridge | v3/crates/effect_runtime/src/v2/**, app.rs, pipeline.rs, chan.rs, runtime_bridge.rs | full |

Action: refire the graph/store agent on a tighter scope (just runtime_graph.rs
+ mounted_query.rs + memo*.rs) if the items below leave questions unanswered.
The truncated findings are referenced by number in the agent's later prose
(#1-#10) but bodies were not delivered.

---

## 1. Problem statement

The v4 compile/lower trait surface has drifted: 3 of 4 `OperatorDef::lower*`
methods are unused, `Op::classify` was specced but never wired, `key_terms`
has no readers, `DslInterp` carries dead back-compat fields, and `rule` is
lowered through five forks of `crate::sql::rule_*_pipe` outside the registry.

The v3 effects lib carries a parallel "value-track" surface
(`insert_value`/`set_edge_value`/`EmitValue`/`RuntimeValue`) with zero v4
callers, plus `effect_dispatch.rs` and `query.rs` modules used only by v2
tests. v3 and v4 each declare their own `QuiescenceError`, `replace_supports`,
and incoming-edge traversal.

`v4/src/app.rs::run_fused_sql` downcasts `dyn FactStore` to `SqliteFactStore`
to open a transaction, the only call site that reaches past the bridge.

Banned identifiers and prose (`load-bearing`, `substrate`, `regime`) appear
in 8 file:line positions. Vocabulary mismatch: `DiagSink`/`ProbeSink` violate
the "rule is a function, write = return/yield" rule; `Sink` is the
back-pressure word, these are fire-and-forget writers.

---

## 2. Findings tables

### 2.1 Banned-word hits

| file:line | hit | proposed |
|---|---|---|
| v3/crates/effect_runtime/src/lib.rs:69 | load-bearing | primary |
| v3/crates/effect_runtime/src/v2/memoize.rs:42 | substrate | dispatch |
| v3/crates/effect_runtime/src/v2/query.rs:22 | substrate | runtime |
| v3/crates/effect_runtime/src/v2/component.rs:18,62,66 | substrate | runtime |
| v3/crates/effect_runtime/src/v2/README.md | substrate (multiple) | runtime |
| v4/src/pipeline.rs:71 | substrate | cleanup |
| v4/src/compile/lower/ops.rs:594 | substrate | grammar |
| v4/src/compile/lower/ops.rs:1812 | regimes | paths |

No `provenance` in scope.

### 2.2 Vocabulary (rule-is-a-function ban)

| file:line | name | rename |
|---|---|---|
| v3/.../v2/diag.rs | `DiagSink`, `NoopDiagSink` | `DiagWrite`, `NoopDiagWrite` |
| v3/.../v2/probe.rs:30,35,43 | `ProbeSink`, `NoopProbeSink`, `BufferProbeSink` | `ProbeWrite`, `NoopProbeWrite`, `BufferProbeWrite` |

~30 call sites total.

### 2.3 Compile/lower findings

| # | finding | files |
|---|---|---|
| C1 | `Op::classify` trait dead in prod; tests-only. Fold onto `OperatorDef::classify`; `fuser::classify_op` stops name-switching | liftable.rs:148, fuser.rs:147, ops.rs:1606-1772 |
| C2 | `rule` lowered through 5 forks of `sql::rule_*_pipe` outside the registry; only head-decl branch hits `RuleDef` | walk.rs:419-541, ops.rs:185-252 |
| C3 | `OperatorDef` has 4 lower methods; 3 are unused defaults | op_def.rs:256-307, registry.rs:108 |
| C4 | `key_terms` has 3 overrides, 0 readers in compile scope | op_def.rs:252, ops.rs:877/1237/1356 |
| C5 | `DslInterp.mode`, `.field` dead back-compat fields | op_def.rs:131-149 |
| C6 | `is_ident` reimplemented 3x; `is_caps_ident` dead by policy | walk.rs:946, binding_graph.rs:462, fuser.rs:354/368 |
| C7 | `collect_rule_decl*` written 3x | walk.rs:147, binding_graph.rs:67, :772 |
| C8 | Two `${ident}` scanners | binding_graph.rs:1054, op_def.rs:329 |
| C9 | `LowerCtx` mixes Cell / RefCell / Arc<Mutex> per author; `rules` mutex is unjustified | ctx.rs:18-111 |
| C10 | `walk_pipe::EXTERNAL_SOURCE_OPS` hardcodes 8 unregistered op names | walk.rs:197-201 |
| C11 | `find_host_hole_outside_quotes` `#[allow(dead_code)]` | walk.rs:918 |
| C12 | `_unused(c: &CallConstraint)` allow(dead_code); half of `TypeLattice/CaptureSource/ReadCtx/Fanout/CaptureInfo/CaptureConstraint/UnionFind` unread by fuser | fuser.rs:1050, binding_graph.rs:497-635 |
| C13 | `RuleInvokeValue::Term` variant constructed nowhere | rule.rs:140 |
| C14 | `v2_ops.rs` is a misnomer in v4; should split into `compile/lower/ops/<name>.rs` colocated with the Def | v2_ops.rs:1-3953 |
| C15 | Bench-leftover Components without `OperatorDef`: `SinglePathComponent`, `CountComponent`, `FormatComponent`, `TrimComponent`, `BasenameComponent`, `DirnameComponent`, `LineComponent`, `MultiAstNmComponent`, `RepoFromTermComponent` | v2_ops.rs:1260,1361,1403,1481,1553,1582,1612,1643,2740 |

### 2.4 Effects lib + bridge findings

| # | finding | files |
|---|---|---|
| E1 | `HybridQueue::pull_runnable_batch_for` inherits queue.rs default which discards `(pipe_hash, instance_id)`. Latent correctness bug | v2/hybrid_queue.rs, v2/queue.rs:151-160 |
| E2 | Fact-store value-track dead: `insert_value`, `set_edge_value`, `edge_values`, `replace_active_child`, `active_child`, `EmitValue`, `RuntimeValue`, `RUNTIME_VALUE`, `RUNTIME_EDGE_VALUE` | v2/fact_store.rs, v2/runtime_graph.rs:213-229,294-298,300-304,306-488,944,964 |
| E3 | `effect_dispatch.rs` (155L) and `query.rs` (231L) unused by v4; only v2 tests | full modules |
| E4 | `app.rs::run_fused_sql` downcasts `dyn FactStore` to `SqliteFactStore`; only site reaching past the bridge | app.rs:833-1175, app.rs:1736 |
| E5 | v3↔v4 `replace_supports` duplicated; v3 has no caller | v2/runtime_graph.rs:870, v4/src/runtime_graph.rs:1547 |
| E6 | `QuiescenceError` declared twice | v2/runtime_graph.rs:263, v4/src/runtime_graph.rs:148 |
| E7 | `incoming_edges` reimplemented as `incoming_subscribers`; v4 should call `edges_where(Some("subscribe"), ...)` | v2/runtime_graph.rs:1052, v4/src/runtime_graph.rs:1701 |
| E8 | Generic `RuntimePut` superseded by concrete `SprfSubscribe`/`SprfSupportRows` | v2/runtime_graph.rs:306-488, v4/src/runtime_graph.rs:362-431 |
| E9 | 4 near-identical `SprfState` constructors differ only by `FactStore`/`Queue` pair | app.rs:546-605 |
| E10 | `resume_mounted` mixes `next_instance_id` and `runtime_graph.run_epoch` as a generation source | app.rs:650-668 |
| E11 | `DirtyOwner.job_id: String` is a hex string never compared; `[u8; 32]` would skip per-row encode + 64-byte alloc | v2/runtime_graph.rs:241 |

### 2.5 Graph/store findings (partial, agent truncated)

| signal | files |
|---|---|
| compact-source sqlite sidecar opens a second connection on the same DB file; `_repo_state` lives on it | runtime_graph.rs:1752+ |
| `_memo` schema declares `out_rows`, `out_keys`; rows persisted empty | memo.rs:124-125, :381-382 |
| `MemoVal.dep_fp`, `computed_gen` written but never read | memo.rs:291-296, memo_seam_impl.rs:343-352 |
| `dirty_tables_for_sql_outputs` reachable only via `runtime_replay.rs:73 → app.rs:708 drain_graph_jobs`; if eclipsed by `dirty_source.rs`, the replay path + `record_runtime_sql_mount`'s snapshot become deletable | runtime_replay.rs, mounted_query.rs:131-183 |
| `compact_sources.is_some()` mode toggle scattered with subtly different semantics | runtime_graph.rs:592-661, 788-794, 838-848, 905-921, 1275-1278 |
| `WriteStats` only records the compact path | telemetry |

Marked fine by the agent (no changes): `cursor_codec.rs`, `source_clock.rs`,
`source_index.rs`, `runtime_bridge.rs`, `fact.rs::FactWrite` (full_extent
flag justified), `runtime_graph.rs` NodeKind impls and typed aliases,
`fixpoint.rs`.

---

## 3. Phase ordering

```
Phase 1  banned-word + dead-by-policy deletes (independent, ~1h)
   |
   +-> Phase 2  compile/lower trait collapse  (depends on C4, C5, C12 deletes)
   |
   +-> Phase 3  v3 fact-store value-track + v3↔v4 dup folds (independent of compile)
   |              |
   |              +-> Phase 5  FactSqlExec + app.rs constructor collapse
   |
   +-> Phase 4  Sink → Write rename (independent, runs anytime)

Phase 6  v2_ops.rs split + colocate Def+Component   DEFERRED until callable-value merges
Phase 7  rule-shapes through RuleDef via CallSite   DEFERRED until callable-value merges
Phase 8  Graph/store agent refire + acted-on follow-ups   DEFERRED until graph findings re-collected
```

Phases 1, 3, 4 are independent and can land in any order. Phase 2 needs the
Phase 1 deletes to land first to avoid editing soon-to-be-removed code.

Callable-value is the high-velocity area; phases 6 and 7 wait so we are not
rebasing against an in-flight grammar change.

---

## 4. Phase 1 — Deletes and renames

### 4.1 Banned-word find/replace

Mechanical edits per table 2.1.

```text
// pseudo:
//   sed-style replace at each file:line, one diff per file
//   then `cargo check -p effect_runtime && cargo check -p sprefa`
```

Risk: zero. Identifier-free string changes in prose/comments and `lib.rs:69`
(check whether that one is in a `///` doc or code; if code, follow the type
through callers — likely cosmetic).

### 4.2 Compile dead code (C4, C5, C11, C12, C13)

For each, delete the declaration and any dead overrides; run the full test
suite.

```rust
// C4: OperatorDef::key_terms
fn key_terms(&self) -> &[&'static str]      // delete
//           overrides at ops.rs:877, 1237, 1356  delete
//   pseudo: confirm zero readers in v4/src outside memo_seam_impl
//           which keeps its own copy; nothing else moves

// C5: DslInterp.mode, DslInterp.field
pub struct DslInterp {
    pub name: Arc<str>,
    pub range: ByteRange,
    pub kind: InterpKind,
    // pub mode: InterpMode,   delete
    // pub field: Arc<str>,    delete
}
//   pseudo: update default_plain_dsl_parse to stop populating
//           every consumer already matches on .kind, no fix-up needed

// C11: walk.rs:918 find_host_hole_outside_quotes  delete

// C12: fuser.rs:1050 _unused(c: &CallConstraint)  delete
//      decide on binding_graph.rs:497-635 partial graph:
//        either delete the unread parts (ReadCtx, Fanout::Many, CaptureConstraint,
//        crosses_scope) or keep and document why; recommendation: delete

// C13: RuleInvokeValue::Term variant
pub enum RuleInvokeValue {
    Literal(Arc<str>),
    Value(Value),
    // Term(Arc<str>),  delete unless wired before merge
}
```

### 4.3 EXTERNAL_SOURCE_OPS cleanup (C10)

```rust
// walk.rs:197-201 currently:
const EXTERNAL_SOURCE_OPS: &[&str] = &[
    "fs", "read", "ast", "glob", "repo", "path", "lsp", "http", "ghcache",
    "sg", "grep", "find", "walk", "stat", "git", "dir", "file",   // unregistered
];

// pseudo: drop the 8 unregistered names; the runtime list is whatever
//         the registry says it is. Leave the constant as a transitional
//         seam; finding C-piggyback below will replace it with
//         OperatorDef::is_external_source().
```

### 4.4 v3 fact-store value-track (E2)

Delete the whole value-track surface in one diff. Confirm no callers first:

```bash
rg --files-with-matches '(insert_value|set_edge_value|edge_values|replace_active_child|active_child|EmitValue|RuntimeValue|RUNTIME_VALUE|RUNTIME_EDGE_VALUE)\b' v4/ v3/
```

If only v3 tests show up, those tests delete with the surface.

```rust
// v2/fact_store.rs: delete the value column family + methods
// v2/runtime_graph.rs:213-229, 294-298, 300-304, 306-488, 944, 964: delete
// re-export surface in v3/lib.rs: drop EmitValue, RuntimeValue
```

### 4.5 v3 lab-only modules (E3)

```text
// either delete v3/.../v2/effect_dispatch.rs and v3/.../v2/query.rs outright,
// or move them under v3/crates/effect_runtime/src/lab/ behind a non-default
// feature flag. Recommendation: delete; nothing in v4 reaches them.
```

### 4.6 HybridQueue forwarder bugfix (E1)

Independent correctness fix; ship in Phase 1.

```rust
// v3/.../v2/hybrid_queue.rs
fn pull_runnable_batch_for(
    &self,
    pipe_hash: PipeHash,
    instance_id: InstanceId,
    gt: Generation,
    n: usize,
) -> Vec<QueueRow<N>> {
    let hot = self.hot.pull_runnable_batch_for(pipe_hash, instance_id, gt, n);
    if !hot.is_empty() { return hot; }
    self.cold.pull_runnable_batch_for(pipe_hash, instance_id, gt, n)
}
//   pseudo: forward both scoped lookups to hot then cold, matching the
//           pattern used for pull_runnable_batch in the same file
```

Add a test that drives `expand` with two `PipeInstance`s sharing one
`HybridQueue` and asserts each instance only sees its own work.

### 4.7 Make queue trait defaults loud (E1 follow-up)

Defaults at v2/queue.rs:99, 123, 127, 138, 151, 165, 186 silently produce
broken behavior in any new impl that forgets to override. Convert each to
`unimplemented!("backend X: implement {method}")` or document the
opt-in/opt-out semantics explicitly. Recommendation: loud defaults.

---

## 5. Phase 2 — Compile/lower trait collapse

Depends on Phase 1 deletes (C4, C5, C11, C12, C13 gone). Touches the active
lower path; needs careful test coverage.

### 5.1 OperatorDef lower-method collapse (C3)

```rust
// before
trait OperatorDef {
    fn lower(&self, ctx, args) -> Result<Pipe<Cursor>, LowerError>;
    fn lower_with_chain(&self, ctx, args, chain_pos) -> Result<Pipe<Cursor>, LowerError> {
        self.lower(ctx, args)
    }
    fn lower_call(&self, ctx, call) -> Result<Pipe<Cursor>, LowerError> {
        self.lower(ctx, call.into_args())
    }
    fn lower_call_with_chain(&self, ctx, call, chain_pos) -> Result<Pipe<Cursor>, LowerError> {
        self.lower_call(ctx, call)
    }
}

// after
struct CallInputs<'a> {
    flow:       &'a [Cursor],
    args:       &'a [CallArg],          // positional + keyword
    block:      Option<&'a PipeAst>,
    dsl:        Option<&'a DslBody>,
    chain_pos:  usize,
    call_span:  ByteRange,
}
trait OperatorDef {
    fn lower(&self, ctx: &LowerCtx, call: CallInputs<'_>)
        -> Result<Pipe<Cursor>, LowerError>;
}
//   pseudo:
//     RuleDef::lower    inspects call.chain_pos + call.dsl to pick branch
//     GhPrsDef::lower   reads call.args.iter().find(|a| a.kw == Some("repo"))
//     all other Defs    ignore chain_pos
//     registry.rs:108   passes CallInputs through unchanged
```

Override sites to migrate: `RuleDef::lower_with_chain` (ops.rs:185),
`GhPrsDef::lower_call` (ghcache.rs:118). Two real overrides become two real
overrides; the three shim methods disappear from the trait.

### 5.2 `Op::classify` fold-in (C1)

```rust
// liftable.rs:148  delete trait Op
trait OperatorDef {
    fn classify(&self, call: &CallInputs<'_>) -> Liftable { Liftable::Opaque }
}
//   pseudo:
//     FsDef::classify       Liftable::Stream
//     AstDef::classify      Liftable::Stream
//     ReDef::classify       Liftable::Stream
//     WhereDef::classify    Liftable::Filter
//     RuleDef::classify     match call.chain_pos { 0 => Liftable::Stream, _ => Liftable::Opaque }
//     fuser.rs:147 classify_op = registry.get(name)?.classify(&call_inputs)
//
//     delete: ops.rs:1606-1772 FsOp/AstOp/ReOp/WhereOp/RuleCallOp wrappers,
//             their new_for_test_* constructors, v4/tests/liftable_classify_target.rs
//             (or repoint the test at the OperatorDef impls)
```

### 5.3 EXTERNAL_SOURCE_OPS → trait method (C10 continuation)

```rust
trait OperatorDef {
    fn is_external_source(&self) -> bool { false }
}
//   pseudo:
//     FsDef/ReadDef/AstDef/GlobDef/RepoDef/PathDef/LspDef/HttpDef/GhPrsDef
//        override to true
//     walk.rs:197 EXTERNAL_SOURCE_OPS constant deleted
//     walk_pipe predicate becomes:
//        !p.steps.iter().any(|op| reg.get(&op.name).map_or(false, |d| d.is_external_source()))
//
//     alternative: if classify().is_stream() already implies external source,
//                  fold this into classify() and drop the separate predicate.
//                  recommendation: do this; one trait method, two callers.
```

### 5.4 LowerCtx mutability cleanup (C9)

```rust
// before  ctx.rs:18-111
pub struct LowerCtx {
    rules: Arc<Mutex<HashMap<Arc<str>, Rule>>>,
    apply_seq: Arc<AtomicU64>,
    query_site: Arc<AtomicU64>,
    current_call_span: Cell<Option<ByteRange>>,
    pipe_full_extent: Cell<bool>,
    scope_path: RefCell<Vec<Arc<str>>>,
    col_types: RefCell<HashMap<...>>,
    bindings: HashMap<...>,
    // + 6 input Arcs
}

// after
pub struct LowerCtx {
    // inputs (cheap clones, no interior mutability)
    store, interner, root, sprf_dir, probe, telemetry, config, sprf_store,
    warm_skip, warm_slice,

    // single-threaded mutable state, all RefCell
    rules: RefCell<HashMap<Arc<str>, Rule>>,
    bindings: RefCell<HashMap<...>>,        // or keep pass-by-value via with_binding
    scope_path: RefCell<Vec<Arc<str>>>,
    col_types: RefCell<HashMap<...>>,

    // single-threaded counters
    apply_seq: Cell<u64>,
    query_site: Cell<u64>,
    current_call_span: Cell<Option<ByteRange>>,
    pipe_full_extent: Cell<bool>,
}
//   pseudo: register_rule borrows mut once; downgrade Arc<Mutex<>> + Arc<AtomicU64>
//   confirm: no compile path is threaded; LSP background reanalyze creates a
//            fresh LowerCtx per invocation
```

### 5.5 Compile dedup (C6, C7, C8)

```rust
// new file v4/src/compile/ident.rs
pub fn is_ident(s: &str) -> bool { ... }
pub fn is_dotted_ident(s: &str) -> bool { ... }   // a.b.c

// pseudo:
//   walk.rs:946 is_ident       delete; call ident::is_ident
//   binding_graph.rs:462       delete; call ident::is_ident
//   binding_graph.rs:475 is_rule_name = ident::is_dotted_ident
//   fuser.rs:354 is_ident_trim = ident::is_ident on .trim()
//   fuser.rs:368 is_caps_ident  delete entirely (no callers should rely on caps)
//                fuser.rs:338 TermBind::Bind gate becomes is_ident, not caps-restricted

// collect_rule_decls consolidation
// v4/src/compile/binding_graph.rs
pub struct RuleDecl {
    pub cols: Vec<Arc<str>>,
    pub has_body: bool,
}
pub fn collect_rule_decls(program: &[PipeAst]) -> HashMap<Arc<str>, RuleDecl>;
// pseudo:
//   walk.rs:147 collect_rule_decl_cols  delete; call binding_graph::collect_rule_decls
//                                              and project .cols
//   binding_graph.rs:67 keeps the single implementation
//   binding_graph.rs:772 rule_decl_name stays as the projection helper

// dollar-ident scanner consolidation
// binding_graph.rs:1054
fn scan_dollar_idents(raw: &str) -> Vec<Arc<str>> {
    default_plain_dsl_parse(raw)
        .into_iter()
        .filter_map(|i| matches!(i.kind, InterpKind::Term { mode: Read, .. })
                        .then_some(i.name))
        .collect()
}
```

---

## 6. Phase 3 — v3 ↔ v4 dup folds (independent of Phase 2)

### 6.1 QuiescenceError (E6)

```rust
// v2/runtime_graph.rs:263 declares QuiescenceError
// v4/src/runtime_graph.rs:148-194 redeclares it

// pseudo:
//   delete v4's enum
//   add: use effect_runtime::v2::runtime_graph::QuiescenceError;
//   if v4 adds variants v3 doesn't have, lift them into v3 (the graph lives
//   there per the 2026-05-16 genericization initiative)
```

### 6.2 replace_supports (E5)

```text
// v2/runtime_graph.rs:870 has no caller (the v4 site at v4/src/runtime_graph.rs:1547
// is a parallel impl that took over)
//
// pseudo: delete v3's replace_supports + helper types VisibleDelta (:294)
//         and SubResult (:300). Confirm with rg first.
```

### 6.3 incoming_edges → edges_where (E7)

```rust
// v4/src/runtime_graph.rs:1701  incoming_subscribers
// becomes
fn incoming_subscribers(&self, src: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    self.edges_where(Some("subscribe"), None, Some(src), None)
}
//   pseudo: v3's incoming_edges at v2/runtime_graph.rs:1052 stays as the
//           generic accessor; v4's named helper becomes a one-line wrapper
```

### 6.4 RuntimePut → SprfSubscribe (E8)

```text
// v3/.../v2/runtime_graph.rs:306-488 four RuntimePut impls
//   apply() downcasts ctx.runtime::<FactRuntimeGraph<R>>() which v4 never
//   installs (v4 installs RuntimeGraph). Dead code.
//
// pseudo: delete the four impls + RuntimePut trait if no other v3 caller.
//         v4's SprfSubscribe / SprfSupportRows at v4/src/runtime_graph.rs:362-431
//         are the live concrete types.
```

### 6.5 DirtyOwner.job_id → [u8; 32] (E11)

```rust
// v3/.../v2/runtime_graph.rs:241
pub struct DirtyOwner {
    pub kind: Arc<str>,
    pub job_id: [u8; 32],     // was String hex
    pub gen: Generation,
}
//   pseudo: storage column becomes BLOB(32) NOT NULL UNIQUE; SQL builder skips
//           the hex encode; Rust callers compare bytes directly
//   migration: pre-stable, drop+recreate; or add a one-shot upgrade that
//              re-hexes legacy rows on first open
```

Defer if this collides with the in-flight memory-control plan.

---

## 7. Phase 4 — Sink → Write rename

Independent, mechanical, runs anytime.

```text
// renames per table 2.2
//   DiagSink         → DiagWrite
//   NoopDiagSink     → NoopDiagWrite
//   ProbeSink        → ProbeWrite
//   NoopProbeSink    → NoopProbeWrite
//   BufferProbeSink  → BufferProbeWrite
//
// call sites to update: v4/src/app.rs:30, app.rs:170, lsp.rs:39, plus ~25 more
// approach: one PR per type to keep diffs reviewable; or one mechanical rename
//           commit with `cargo check && cargo test` at each step
```

---

## 8. Phase 5 — FactSqlExec trait + app.rs collapse

Depends on Phase 3 not strictly, but cleaner if v3 type churn settles first.

### 8.1 FactSqlExec trait (E4)

```rust
// new in v3/crates/effect_runtime/src/v2/fact_store.rs
pub trait FactSqlExec {
    fn with_connection<T>(&self, f: &mut dyn FnMut(&mut Connection) -> T) -> Option<T>;
}
impl<R: Row> FactSqlExec for SqliteFactStore<R> {
    fn with_connection<T>(&self, f: &mut dyn FnMut(&mut Connection) -> T) -> Option<T> {
        Some(f(&mut self.conn.lock().unwrap()))
    }
}
// impl FactSqlExec for MemFactStore: None.  // fall-through path

// v4/src/app.rs:833-1175 (run_fused_sql) before:
//   let Some(sqlite) = (&*facts as &dyn Any).downcast_ref::<SqliteFactStore<...>>() else { ... };
//   let mut conn = sqlite.conn.lock().unwrap();
//   ...fused SQL...

// after:
let Some(rows) = facts.with_connection::<u64>(&mut |conn| {
    /* fused SQL with conn */
}) else {
    return false;   // MemFactStore path falls through
};

// app.rs:1736 incremental gate uses the same pattern
```

State impact: `with_connection` takes the same lock the downcast path took.
No new contention. The `Any` round-trip + `mem::transmute`-shaped downcast
disappears.

### 8.2 SprfState constructor collapse (E9)

```rust
// v4/src/app.rs:546-605 currently has four near-identical constructors
//   for {Mem,Sqlite} × {MemQueue, SqliteQueue}

// after
impl SprfState {
    pub fn builder() -> SprfStateBuilder { SprfStateBuilder::default() }
}
pub struct SprfStateBuilder {
    facts: Option<Arc<dyn FactStore<Row = ...>>>,
    queue: Option<Arc<dyn QueueBackend<...>>>,
    config: Config,
    telemetry: Option<...>,
}
impl SprfStateBuilder {
    pub fn facts(mut self, f: Arc<...>) -> Self { ... }
    pub fn queue(mut self, q: Arc<...>) -> Self { ... }
    pub fn build(self) -> Result<SprfState, BuildError> { ... }
}
//   pseudo: defaults match the now-current "in-memory SQLite" default (commit 7b58db9)
//           three callers in main, test fixtures, and lsp boot
```

### 8.3 resume_mounted generation unification (E10)

```rust
// v4/src/app.rs:650-668
// before: reads *self.next_instance_id as a generation
// after:  reads self.runtime_graph.run_epoch()

//   pseudo: keep next_instance_id as the InstanceId allocator only;
//           generation comes from the unified clock seam
//   blocker: see plans/2026-05-19-clock-seam-invariants-plan.md
//            land that plan first if not already
```

---

## 9. Phase 6 — v2_ops.rs split (DEFERRED)

Wait for `feat/callable-value` to merge to avoid rebase pain.

```text
// target layout
v4/src/compile/lower/ops/
   mod.rs        // re-exports + the small Defs
   fs.rs         // FsDef + FsComponent
   ast.rs        // AstDef + AstNmComponent (+ AstYamlDef + ...)
   re.rs         // ReDef + ReComponent
   sh.rs         // ShDef + ShComponent
   where_.rs     // WhereDef + WhereComponent
   path.rs       // PathDef + ...
   ...
   rule.rs       // RuleDef + sql::rule_*_pipe private helpers (post-Phase 7)

// pseudo:
//   for each (Def, Component) pair in v2_ops.rs and ops.rs, move both into
//   one file. Confirm the Component is registered through the Def.
//   delete bench leftovers per C15 first.
//
// rename: v2_ops.rs name dies. Nothing left is "v2"-specific in v4.
```

---

## 10. Phase 7 — Rule shapes through RuleDef (DEFERRED)

Wait for `feat/callable-value` to merge.

```rust
// before  walk.rs:419-541 has 5 forks:
//   "rule" && chain_pos >= 1 && block.is_none()   sql::rule_write_pipe
//   r?(...)                                       sql::rule_table_call_pipe
//   bare with body                                sql::rule_body_call_pipe
//   bare without body                             sql::rule_apply_write_pipe
//   rev?(...)                                     sql::rule_table_call_pipe

// after
struct CallSite<'a> {
    name: &'a str,
    chain_pos: usize,
    predicate: bool,       // r?(...)
    force: bool,           // r!(...)
    apply: bool,           // bare without body
    declared_rule_cols: Option<&'a [Arc<str>]>,
}
impl OperatorDef for RuleDef {
    fn lower(&self, ctx: &LowerCtx, call: CallInputs<'_>) -> ... {
        match (call.chain_pos, call.predicate, call.has_block) {
            (0, _, true)   => self.lower_decl(ctx, call),
            (_, true,  _)  => self.lower_table_call(ctx, call),
            (_, false, true)  => self.lower_body_call(ctx, call),
            (_, false, false) => self.lower_apply_write(ctx, call),
        }
    }
}
//   pseudo: sql::rule_*_pipe become private impl details of rule.rs
//           walk.rs::walk_op shrinks by ~120 lines
//           the special-case rev? arm at walk.rs:524 disappears; RevDef::lower
//           declares unconditionally as it already does at ops.rs:2283
```

Migration risk: high. This is the active grammar area. Land after callable-value
ships and after Phase 5.1 (the unified `OperatorDef::lower` shape).

---

## 11. Phase 8 — Graph/store follow-ups (DEFERRED on agent refire)

Refire the graph/store agent with scope limited to `runtime_graph.rs +
mounted_query.rs + memo*.rs` to get the truncated top-findings block. Open
items already in hand from the open-questions section:

| # | item | preliminary direction |
|---|---|---|
| G1 | compact-source sqlite sidecar uses 2nd connection on same DB file | collapse onto one connection; sidecar becomes a table not a database |
| G2 | `_memo.out_rows`/`out_keys` persisted empty | drop columns, one-shot migration |
| G3 | `MemoVal.dep_fp`/`computed_gen` never read | confirm with user: future Phase-N validity check, or retire |
| G4 | `dirty_tables_for_sql_outputs` eclipsed by `dirty_source.rs`? | if yes, delete `runtime_replay.rs` + `record_runtime_sql_mount`'s snapshot persistence |
| G5 | `compact_sources.is_some()` mode toggle | introduce `RuntimeMode` enum or `SubscriptionSidecar` trait |
| G6 | `WriteStats` narrow scope | confirm scope, expand or document |

User decisions needed before Phase 8 lands. See open questions.

---

## 12. Lifetimes table

For state-bearing types touched by this plan:

| type | location | lifetime | wrapper |
|---|---|---|---|
| `LowerCtx` | ctx.rs | per compile invocation, single-threaded | mostly `RefCell` after Phase 2 |
| `LowerCtx.rules` | ctx.rs | per LowerCtx | `RefCell<HashMap<...>>` (was `Arc<Mutex<...>>`) |
| `SprfState` | app.rs | process lifetime | builder constructor |
| `RuntimeGraph` | v4/runtime_graph.rs | process lifetime | shared `Arc`, internal locking |
| `FactStore` (dyn) | app.rs | process lifetime | `Arc<dyn FactStore>` |
| `HybridQueue.hot` | v3/hybrid_queue.rs | per queue instance | inner mem queue + sqlite cold |
| `RuleDecl` | binding_graph.rs (post-Phase 2) | per `collect_rule_decls` return | owned by caller |
| `CallInputs` | op_def.rs (post-Phase 2) | one `lower` call | borrows from walker |
| `CallSite` | rule.rs (post-Phase 7) | one `lower` call | nested inside `CallInputs` |

---

## 13. Storage layout deltas

| Phase | table/struct | before | after |
|---|---|---|---|
| 4.4 | `RuntimeValue` rows | column family present | removed (v2/fact_store.rs) |
| 4.4 | `RUNTIME_VALUE`, `RUNTIME_EDGE_VALUE` constants | exist | deleted |
| 6.5 | `dirty_owners` table | `job_id TEXT NOT NULL UNIQUE` | `job_id BLOB(32) NOT NULL UNIQUE` |
| G2 | `_memo` table | `out_rows`, `out_keys` columns | columns dropped (migration) |
| Phase 5.1 | none | downcast inside `app.rs::run_fused_sql` | trait dispatch through `FactSqlExec::with_connection` |

---

## 14. Sequence of reads/writes per touched operation

### 14.1 `run_fused_sql` (Phase 5.1)

```text
before:
  1. app.rs:833  downcast facts: &dyn Any → &SqliteFactStore
  2. lock conn
  3. begin tx
  4. exec fused SELECT/INSERT
  5. commit
  6. update sprf_strings

after:
  1. facts.with_connection(|conn| {
       2'. begin tx
       3'. exec fused SELECT/INSERT
       4'. commit
       5'. update sprf_strings
       return rows_affected
     }) -> Option<u64>
  2. None branch falls through to legacy non-SQL path
```

### 14.2 `OperatorDef::lower` (Phase 2)

```text
before (5.1):
  registry.rs:108  pick Def by name
                   call lower_call_with_chain → default → lower_call → lower
                   each layer reformats args

after:
  registry.rs:108  pick Def by name
                   build CallInputs once at the walker
                   call lower(ctx, call_inputs)
  RuleDef.lower    inspects call.chain_pos / has_block / predicate
                   dispatches to lower_decl | lower_table_call | lower_body_call | lower_apply_write
                   (all four private to rule.rs)
```

### 14.3 `incoming_subscribers` (Phase 6.3)

```text
before:
  v4/runtime_graph.rs:1701  manual scan of edges table

after:
  v4/runtime_graph.rs:1701  delegate to edges_where(Some("subscribe"), None, Some(src), None)
                            which is the existing generic v3 accessor
```

---

## 15. Uniqueness conditions

| Constraint | Where enforced | Notes |
|---|---|---|
| `OperatorDef::name()` unique per registry | registry.rs:67 | one Def per op name; current invariant, unchanged |
| `RuleDecl` name unique per program | binding_graph.rs (post-Phase 2) | `collect_rule_decls` returns `HashMap` keyed by name |
| `dirty_owners.job_id` unique per `(owner, source, gen)` | v3 schema | unchanged by [u8;32] rename |
| `Pipe::instance_id` content-addressed (`stable_pipe_identity`) | app.rs:2103 | unchanged; memory-control plan owns this |
| `CallInputs.args` ordering | walker emits them in source order | positional args precede keyword args within `args` |

---

## 16. Tests to add

| Phase | test |
|---|---|
| 4.6 | shared `HybridQueue` across two `PipeInstance`s; each instance only pulls its own rows |
| 5.1 | `MemFactStore` path of `run_fused_sql` returns the fall-through error path (was unreachable before via downcast `else { panic }`); `SqliteFactStore` path identical to before |
| 5.2 | `OperatorDef::lower` over a captured fixture: walker output matches old `lower_with_chain` output across ~10 representative pipelines |
| 5.4 | `LowerCtx` borrows do not panic under nested lowering (block-inside-block) |
| 6.1 | `QuiescenceError` From-conversion preserves variant + payload |
| 6.5 | dirty-owner roundtrip with `[u8;32]` job_id (insert, query, delete) |

---

## 17. Open questions for the user

1. `Op::classify` (liftable.rs:148) — design intent. Wire it onto
   `OperatorDef` per finding C1, or delete the trait outright? If wired, the
   five test wrappers (`FsOp`/`AstOp`/`ReOp`/`WhereOp`/`RuleCallOp`) go away.
2. `key_terms` was specced for the retraction/memo layer per
   `docs/v4-retraction-fixpoint-plan.md`. Still alive? If yes, leave the trait
   method but mark the dead overrides; if no, delete (C4).
3. `walk.rs` rule-shape special cases — happy to move all 5 forks onto
   `RuleDef::lower` via `CallSite` (Phase 7), or do you want walker to stay as
   the dispatcher? Phase 7 is deferred either way.
4. `LowerCtx::rules` Arc<Mutex<>> — is there any threaded compile path
   (LSP background reanalyze?) that I am missing, or can it drop to `RefCell`
   (C9)?
5. `feat/callable-value` merge plan — does it retire `crate::sql::rule_*_pipe`
   free functions, or do they stay as `RuleDef`'s implementation helpers?
   Phase 7 assumes the latter.
6. `v2_ops.rs` rename + split — land as a mechanical move now, or defer until
   callable-value merges? Recommendation: defer.
7. `effect_dispatch.rs` + `query.rs` lab-only — delete outright, or move to
   `effect_runtime::v2::lab` behind a non-default feature (E3)?
8. `Sink → Write` rename timing — Phase 4 churn is small (~30 sites). Pre-
   stable, so now is fine. Confirm?
9. Graph/store agent refire — happy to fire a smaller-scope re-run for the
   truncated top-findings block, or work with the partial data?
10. `MemoVal.dep_fp` and `computed_gen` written but never read — retire, or
    keep for a planned Phase-N validity check (G3)?

---

## 18. Out of scope

Items raised by an agent but deliberately excluded from this plan:

- LSP architecture refactor (large independent debt; covered by
  `plans/2026-05-19-lsp-architecture-plan.md`).
- Type-IR value-space plan (`plans/.../types-in-value-space`, draft on a
  separate worktree).
- Memory-control / instance-leak (
  `plans/2026-05-20-instance-leak-and-memory-control.md`).
- Clock-seam invariants (`plans/2026-05-19-clock-seam-invariants-plan.md`).
- `runtime_bridge.rs` cursor-hash caching perf TODO (a chore, not a refactor).
- `parse.rs:545-570` synthetic-`;` insertion (better fixed at tree-sitter
  grammar level; cross-repo change).

---

## 19. Estimated effort

| Phase | size | one-PR or many |
|---|---|---|
| 1 (deletes + renames + HybridQueue bug) | ~1 day | one PR per logical chunk: banned-word, compile-deletes, v3-value-track, HybridQueue |
| 2 (compile/lower trait collapse) | ~3 days | one PR (touches the lower path; needs full test signal) |
| 3 (v3↔v4 dup folds) | ~1 day | one PR |
| 4 (Sink rename) | ~half day | one PR, mechanical |
| 5 (FactSqlExec + builder + clock unify) | ~2 days | three PRs (FactSqlExec, builder, clock) |
| 6 (v2_ops split) | ~2 days | one mechanical PR after callable-value merges |
| 7 (rule via RuleDef) | ~3 days | one careful PR after callable-value merges |
| 8 (graph/store follow-ups) | TBD on refire | TBD |

Total mainline (1+2+3+4+5): ~7-8 working days assuming no test regressions.
