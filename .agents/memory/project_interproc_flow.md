---
name: project_interproc_flow
description: std/flow.dl shared value-flow base — per-call-site pinned hops, flow_summary models, lambda lift, wire hop; line bases now 1-based everywhere
metadata: 
  node_type: memory
  type: project
  originSessionId: 56424a34-a5a4-4ee1-b2de-df146b4ee420
---

`examples/flow-interproc.dl` + `tests/it/flow_interproc.rs` (main, local/uncommitted
2026-06-29). Unions the intra-procedural `df_edge` lift with an interprocedural
hop into ONE `flow_edge` rel, so `closure(flow_edge)` walks value flow ACROSS
function boundaries — the join that did not exist (df_* was intra-proc only, the
scip_*/call_* graph walked symbols not values). See [[project_doc_comment_spine]]
for the same TypeLang extractor family.

Built as pure DL composition over existing rels, ZERO engine change (the repo's
idiom for flow, like `bench/flow/*.dl`). The interproc hop: an arg feeding a
`call_res` flows into the resolved callee's params.

Key facts that shaped it:
- **Resolution rides `call_edge`, which is already SCIP-preferred** (engine.rs
  ~4158 `scip_name_defs` -> def_file -> sym, single-def name fallback). So the
  cross-fn hop is SCIP-resolved for free; nothing new to wire.
- **Sym-space bridge**: `df_node.fn` is the BARE sym (`file::kind::name`,
  typegraph.rs:468 `mint_sym`); `call_edge`/`call_name`/`type_sig` syms are
  repo-qualified (`repo::file::kind::name`). Bridge with the suffix-containment
  idiom `strip = replace(qual, bare, ""), strip != qual` (same trick the strict
  dataflow test uses).
- **GOTCHA — df_node line base is NOT cross-language consistent**: Rust df_node
  lines are 1-based (proc-macro2), Kotlin/TS are 0-based (tree-sitter/byte). But
  `call_site.line` is 1-based for ALL langs. So a `(file,line)` join between
  df_node `call_res` and `call_site` works for Rust, SILENTLY MISSES for Kotlin
  (off by one). The `nest`/`loop_over` tests never caught this — Rust-only. Fix:
  resolve through `call_edge` (sym->sym, no line) instead of `call_site`+(file,line).

RETURN hop LANDED (the first deferred assist, 2026-06-29): the lift now mints a
`ret` df_node kind at every return point — Rust `Expr::Return` + the block-tail
no-semicolon expr (flow_block now returns `Option<(id,line,col)>`); Kotlin
`jump_expression` + the function_body tail; TS `ReturnStatement`. Each edges the
returned value -> the ret node. The backward DL rule connects a callee's `ret` to
the caller's `call_res` (same call_edge + suffix-bridge as the forward hop), so a
value passed IN, transformed, and RETURNED reaches the caller's call result end to
end. Test `value_flows_back_out_of_a_callee_per_language` (Rust/TS/Kotlin): driver's
`secret` reaches sink's `v` through ident's return. Full suite green (it 305/0/3,
lib 162/0/1). NOT yet lifted: TS arrow expression-body returns `(x) => expr`.

NODE-LEVEL TYPING + TS ARROWS LANDED (finish-off, 2026-06-29):
- New builtin rel `df_param(id, pos)` (DATAFLOW_RELS now [;6]): the param df_node
  id + its positional index. Index counts ONLY typed params (Rust receiver `self`
  skipped via an explicit counter, NOT enumerate) so it aligns with
  `type_sig.pos`, which also drops self. Populated from `DataflowFacts.param_pos`
  (new Vec<(String,u32)>) filled in all four param-seeding sites (Rust
  flow_fn_body, Kotlin kt_flow_fn, TS FunctionDeclaration x2, TS ts_lift_fn).
  Example rule `flow_node_type(node, callee, ty)` binds a specific param NODE to
  its type: df_node param + df_param pos + type_sig(callee,"param",pos,ty). Test
  `typed_view_carries_resolved_param_types` asserts both the function-level and
  node-level views. df_param has a builtin_rel_docs entry (rel_catalog test) and
  appears in README via the builtin-rels generator.
- TS arrow / function-expression consts (`const f = (x) => ...`) are now lifted
  as their own fn scope (params + body + ret) via new helper `ts_lift_fn`,
  dispatched in the VariableDeclaration arm (was the `_ => expr` hole). Expression
  bodies (oxc `expression==true`) wrap the expr as one ExpressionStatement = the
  implicit return. Test `ts_arrow_const_is_lifted_as_a_function`.

LINE BASES NOW 1-BASED EVERYWHERE (roadmap arc, 2026-07-02 — retires the old
"deliberately skipped" ruling AND corrects it: TS was ALREADY 1-based via
line_at, only Kotlin was 0-based). kotlin_dataflow_from bumps node lines + loop
spans together (+1) so nest containment never desyncs — the feared desync only
applied to bumping one side; node IDS keep raw 0-based rows (opaque). Rust
method-call call_res node moved to the METHOD ident's line (call-site extractor
records that line), so multiline builder chains join call_site. Result:
call_node (std/flow.dl) = ONE (file,line) equality; the old dual-offset
`cl = dl + 1` arm (which also false-matched next-line sites) is gone.

POSITIONAL HOP + CTOR DATAFLOW LANDED (main 5ef8486, 2026-07-02): new rels
`df_arg(call, pos, arg)` (0-based slot, method receiver -1, aligned with
df_param.pos) and `df_field(id, field, value)` (struct-literal fields, TS
object-literal properties, Kotlin named args, ".." = spread/FRU base) across
all three lifts. Instantiations = `new` df_node kind w/ type name (Rust struct
literal + capitalized tuple-struct/variant ctor heuristic, TS new/object
literal, Kotlin capitalized ctor); field reads = `member` nodes w/ the name
(Rust Expr::Field + Kotlin navigation were `expr` catch-alls with NO base
edge). flow-interproc.dl + taint.dl forward hop now joins df_arg.pos =
df_param.pos (arg 0 no longer reaches param 1 — position gate in
flow_interproc.rs). flow-ctor.dl = ctor inventory + field-sensitive
field_flow via a new-seeded recursive rule (a closure rel CANNOT be read
unpinned in a rule body — the engine bails; magic-set-style seeding is the
workaround). e2e output parsing gotcha: an empty FIRST column (anonymous
object-literal ty) is eaten by trim_start(); strip only the 2-space indent.

JSX FLOW LANDED (main ac6bebe, 2026-07-02): JSX element = `new` df_node w/
component/tag name + df_field per attribute (spread "..", children under
"children"); component usage = call SITE (host elements skipped) so
call_edge resolves caller->Card and call_name(sym, name) is the INDEXABLE
name handle (use it instead of a suffix test whenever a rule needs
"callee named X"); TS destructured object params mint one param df_node
PER property (var = KEY name, scope binds LOCAL name, shared slot index —
was a total hole, ts_binding_name returned None for patterns).
examples/flow-jsx.dl prop_edge = the name-match hop template (df_field
prop name = param var OR member var). Microservice/cross-repo value flow:
bench/flow/flow_scip.dl proves the spec seam (operationId -> gen_def ->
consumer via SCIP); the service hop = client call arg -> handler param
bridged by a spec-derived rel, pure DL over multi-repo scan + scip_want.

ROADMAP ARC 1-6 LANDED (main, local, 2026-07-02) — all six items:
- **std/flow.dl** = the shared base as a `use` module (call_edge_bare 4-col,
  flow_edge union, call_node); flow-interproc/taint/flow-jsx rebased on it.
  A `use`d module must have NO `?` queries (would perturb the importer's
  query-section order that e2e tests parse positionally).
- **flow_summary(callee,pos)/flow_sanitizer(callee)** = propagation MODELS.
  KEY INSIGHT: an additive summary hop is REDUNDANT — the lift already draws
  a blanket arg->call_res edge for every call. The real semantics is
  SUPPRESSION: a modeled callee keeps only summarized slots (flow_kept/
  flow_cut, stratified negation; sanitizer = zero-slot instance). Free when
  no facts asserted.
- **call_target(call, caller, callee, callee_q)** = PER-CALL-SITE pinned
  resolution (call_node ⨝ call_edge_bare ⨝ call_name). Both interproc hops
  ride it: f(secret);g(benign) no longer cross-talks args or returns (the
  old per-caller hop leaked both ways). PLANNER LESSON: the same pin
  INLINED as a 7-atom rule = ~7s/tick on this repo; factored through the
  4-atom call_target rel = ~0.5s whole graph. stmt_ms found it (add
  `? stmt_ms(rel, ms).` to any program). Residual: callee param->ret merges
  callers (k-CFA cloning out of scope).
- **arg_field_flow** = prop_edge generalized to plain calls (composite arg's
  df_field -> same-named member/param reads in the resolved callee).
- **Lambda lift**: inline lambdas (Rust closures, TS inline arrows/fn-exprs,
  Kotlin lambda literals; trailing-lambda syntax was not even an ARG before)
  lift as own fn scopes: param nodes + df_param slots (Kotlin implicit `it`
  = slot 0), ret node, sym = `<enclosing>::closure::<pos>`; the `closure`
  VALUE node carries the lifted sym in var = the join key. flow_lambda(name,
  lam_pos, src_pos, param_pos) + flow_lambda_ret(name, lam_pos) hops;
  std/flow-collections.dl = name-keyed facts for map/filter/fold/... nest
  loop-matching is ::closure::-prefix aware.
- **flow-services.dl** = the wire hop: spec-seeded service_op (openapi.yaml
  operationId; glob GOTCHA: `**/openapi.yaml` works, brace-set
  `openapi.{yaml,yml}` does NOT match) + op_endpoint (call_name) + arg->param
  / ret->call_res hops. Stub + handler SHARE the op name so single-def
  resolution refuses exactly where the wire hop takes over — that ambiguity
  IS the test fixture's negative-control design.
- Dogfood: taint.dl on this repo 161 -> 9 findings under the per-site pin
  (deleted rows were per-caller cross-talk). Cold derived 1.07s vs 1.4s
  pre-arc baseline with strictly more precision.

Remaining honest precision limits (documented in the .dl headers):
- callee-internal param->ret path merges all callers (no per-site cloning).
- Kotlin named args keep their SOURCE index in df_arg (reordered named args
  misalign positionally; the df_field name row is the accurate one).
- flow_lambda facts are name-keyed, ecosystem-ambiguous by design (JS reduce
  vs Rust fold slot shapes both listed; tune per codebase).

Rust-string test gotcha: a `\n\` continuation in a fixture literal EATS the
next line's leading spaces — silently destroys YAML indentation (spec fixture
scanned as 0 rows). Write YAML fixtures as single-line literals.

Full suite green after the roadmap arc: it 447/0/4, lib 199/0/1.

MEASURED ON THIS REPO (release, cold tick, 2026-07-02) — the honest limits have
a price tag: flow_edge = 471,406 rows (positional-blind hop is quadratic per hub
fn); each interproc rule = ONE 40s / 22s SQL statement (the suffix-bridge
`replace(qual,bare,"")` is unindexable → ~25M replace() evals over call_res
9,849 × call_edge 2,519); `rebuild_derived`'s fixpoint re-runs non-recursive
strata a 2nd time to observe delta=0 (the 40s statement executes TWICE);
UNPINNED `? flow_reach(from,to)` over the 471k-edge graph = unbounded closure
materialization (killed at 4:38 CPU). Families were CHEAP at this scale
(type/call/dataflow refresh 39/93/330ms). Fix directions: positional forward hop
via df_param, an equality bare→qualified bridge rel instead of replace(), pin
the closure queries in the example, one-pass non-recursive strata in the engine.
