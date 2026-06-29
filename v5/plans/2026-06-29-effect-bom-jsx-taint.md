# Plan: Effect BOM + JSX/effect taint + string builtins (zoom 2)

Branch state: this lives on worktree `sprefa-operator-stratum` (off main f305497).
That branch ALSO carries the uncommitted operator-head second-stratum engine fix
(see "Done" below) — commit that separately before/around this arc.

## Done (this session, uncommitted on the branch)
- **operator-head second stratum** (engine.rs): `partition_derived_strata()` +
  `DerivedStrata`; `tick`/`tick_paths` rebuild pre-stratum -> operator evals ->
  post-stratum. A derived rule can now read an `scc`/`node2vec` head (was silent
  empty). Tests in tests/it/scc.rs (2 new). Suite green (lib 165, it 341).
  examples/fuzzy-traits.dl updated (`tsize <- ftrait` now legal).

## Prototypes (scratchpad, proven on a synthetic RTKQ fixture)
- `effect-bom.dl` — effect / reaches_effect / effect_fanin / mutation_into_pure.
- `effect-equation.dl` — per-hook effect_input / effect_transform / effect_output.
- `leaf-classify.dl` — all RTKQ hook kinds + endpoint-op recovery.
- fixture: scratchpad/effbom/src/{app.ts, rtkq.ts, jsx.tsx}.

## Findings (empirical, do not re-derive)
- `call_site(repo, caller, callee, file, line)`; `callee` = LAST segment, so
  `api.endpoints.getUser.useQuery` records `useQuery` — the op name is dropped.
- `call_site.line == df_node.line` for TS (line-base agrees). Rust is 1-based,
  Kotlin/TS 0-based — a mixed repo must bridge via call_edge, not the line join.
- `call_site.caller` is repo-qualified; `df_node.fn` is bare. Bridge with the
  suffix-strip idiom (`strip = replace(qual, bare, ""), strip != qual`), as in
  flow-interproc.dl.
- **JSX is NOT lowered to a call** (proven): `ts_flow_expr` (typegraph.rs:967)
  `_ =>` arm collapses BOTH `JSXElement` and inline arrow/function callbacks to
  one opaque `expr` node. That single arm is the root cause of (a) props not
  flowing into components AND (b) `useEffect(() => m(user))` losing `user`.
  "Tainting effects" and "tainting JSX" are the same fix.
- Closure heads keep the unpinned-read restriction (would materialize V^2); the
  scc/node2vec fix does NOT lift it. Use recursive `reach` for call-graph reach.

## Decision: dl overlay vs Rust (Chris's question)
Not exclusive. Emulate in dl first; harden in Rust if the overlay misses.
- **Route DL** (no engine change): mint synthetic JSX-call + effect-capture
  edges from `match` regex (and CST `node`/`child` if TSX is covered). Pure .dl,
  fast to iterate, crude nesting.
- **Route RUST** (extractor): fix the one `_ =>` arm + extract_calls + ts_lift_fn.
  Robust, SCIP-resolved cross-file.

## Workstream A — string builtins (greenlit, Rust pass-through)
Seams: `lower.rs:46` (fn -> SQL dispatch), `db.rs` (UDF registration, where
`sprf_split` already lives), `typecheck.rs` (arity/type).
Add registered UDFs, thin pass-through to Rust `str`:
```
sprf_lower/upper/lcfirst/trim(s) -> s
sprf_strip_prefix/strip_suffix(s, p) -> s
sprf_starts_with/ends_with/contains(s, p) -> int(0|1)
sprf_slice(s, lo, hi) -> s        sprf_char_at(s, i) -> s
```
- lower.rs: add `name if args.len()==N => Ok(format!("sprf_X({})", ...))` arms.
- db.rs: `conn.create_scalar_function("sprf_X", N, flags, |ctx| ...)`.
- typecheck: register arities alongside split/3, replace/3.
Unblocks: `useGetUserQuery -> getUser` as pure dl (strip `use`/`Lazy` prefix +
`Query`/`Mutation` suffix via replace, then `sprf_lcfirst`).

## Workstream B — Effect BOM example (pure dl, lands now)
Promote scratchpad/effect-bom.dl -> examples/effect-bom.dl. Rels:
```
effect(callee, host, file, line, kind)         # leaf set, classified by suffix
reaches_effect(fn, callee, line, kind)         # recursive reach over call_edge
effect_fanin(callee, line, n)                  # aggregate (post-stratum payoff)
pure(fn) <- fn_all(fn), !hosts_effect(fn)      # hook-free component
mutation_into_pure(host, pure_comp)            # the taint, fn granularity
endpoint_call(op, member, file, line)          # match regex recovers op name
```
Fold endpoint op into the leaf via `hook join endpoint_call on (file,line)`.

## Workstream C — JSX/effect taint, Route DL overlay (no Rust)
examples/effect-taint.dl. Skeleton:
```
jsx_render(comp, file, line)    <- match(/<(?<comp>[A-Z][A-Za-z0-9]*)/).
jsx_prop(comp, prop, var, file, line)
    <- match(/(?<prop>[a-z]\w*)=\{(?<var>[A-Za-z_]\w*)\}/) joined to render line.
effect_capture(var, file, line) <- match useEffect arrow body identifiers.
# synthesize the missing flow: prop var -> its df binding -> into the component
synth_flow(binding_node, comp)
    <- jsx_prop(comp, _, var, f, line), df_node(b, _, var, fn, _, _), <suffix bridge>.
taint(a, b) <- closure(flow_edge UNION synth_flow ...)   # or recursive reach
```
OPEN: probe CST `node`/`child` TSX coverage (node rel is 6-ary:
`node(id, kind, file, lo, hi, ?)`) — if jsx_element/jsx_attribute nodes exist,
walk the real CST instead of regex (handles nesting).

## Workstream D — JSX/effect taint, Route RUST (robust)
typegraph.rs:
- `ts_flow_expr` (967): add `E::JSXElement(j)` arm — callee = tag ident; for each
  `JSXAttribute` with an expression value, `ts_flow_expr` it -> DfEdge into a
  component props/call_res node; handle JSXSpreadAttribute + children.
- TS `extract_calls` (~1065): emit a `call_site`/`call_def` row per JSXElement
  (caller = enclosing fn, callee = tag) so call_edge resolves `Panel->PureCard`.
- `ts_lift_fn` (786, lifts arrow CONSTS today): also lift a function/arrow passed
  as a CALL ARGUMENT (useEffect/map/forEach callback) as its own fn scope; edge
  captured outer vars in. This is "tainting effects" + handlers for free.
Payoff: scip-typescript resolves cross-module; build an `effect_flow` graph;
"recurrent input to an endpoint" = `scc(effect_flow)` (rides the operator-head fix).

## Timing note (Chris asked)
Taint is order-insensitive: a value captured into an effect closure is a tainted
input regardless of render-vs-effect-commit order. Do NOT model execution order
for taint. If a temporal "effects fire post-commit" view is ever wanted, that is
a separate axis on the existing rev/tx spine.

## Sequence
A (greenlit, small, unblocks op recovery) -> B (lands the BOM now) ->
C (dl overlay, proves JSX+effect taint with no engine change) ->
D (Rust, only if the overlay misses on real nesting / cross-file).

## Open questions
- CST node/child TSX coverage (drives C: regex vs real CST walk).
- "pure component" definition: no-hooks heuristic vs `// @pure` marker vs allowlist.
- op -> REST path/verb: derive from name (needs A) OR json-load the generated
  api slice (authoritative url+method per op).
- Branch hygiene: A/D are engine changes -> own worktree off main; B/C are pure .dl.

## Context files to re-read on resume
- src/typegraph.rs:967 `ts_flow_expr` (the `_ =>` arm), :786 `ts_lift_fn`,
  :1065 TS `extract_calls`, :702 `ts_dataflow_from`.
- src/lower.rs:46 (fn->SQL), src/db.rs (`sprf_split` UDF reg).
- src/engine.rs:806 call rel decls, :831 df rel decls, `partition_derived_strata`.
- examples/flow-interproc.dl (flow_edge + suffix bridge).
- scratchpad/{effect-bom,effect-equation,leaf-classify}.dl,
  scratchpad/effbom/src/{app.ts,rtkq.ts,jsx.tsx}.
