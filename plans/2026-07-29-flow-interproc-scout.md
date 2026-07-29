# flow-interproc scout

Base receipt: `git rev-parse HEAD` returned `9eb0d4f1dcd24bd44f6f3552af7a29c75aaadbd4`.

## 1. Consumed-rel inventory

`examples/flow-interproc.dl` contributes `scan`, `closure`, `df_node`,
`call_edge_bare`, `df_param`, and `type_sig` as body inputs. The direct receipts
are `examples/flow-interproc.dl:35-56` and `examples/flow-interproc.dl:63-69`.
`std/flow.dl` adds the shared body inputs `call_edge`, `call_site`, `call_name`,
`df_edge`, `df_arg`, `df_param`, `df_node`, and `df_field` at
`std/flow.dl:28-49`, `std/flow.dl:80-95`, `std/flow.dl:107-135`,
`std/flow.dl:165-183`, and `std/flow.dl:195-205`.

The v5 production locations are:

| relation or operator | v5 producer and receipt | shape used by this port |
| --- | --- | --- |
| `scan` | Source operator documentation and parser: `src/engine/decls.rs:221-225`, `src/parse/ops.rs:29-91`. It selects the file corpus and does not come from the typegraph extractor. | `scan("WORK", "src/**/*.{rs,ts,kt}", p, rev)` at `examples/flow-interproc.dl:37-38`. |
| `df_node` | `collect_dataflow_rows` consumes each language's `extract_dataflow` result at `src/engine/extract/dataflow.rs:187-213`, emits node rows at `src/engine/extract/dataflow.rs:316-347`, and writes `df_node` at `src/engine/extract/dataflow.rs:470-486`. The public schema is `id, kind, var, fn, file, line, col` at `src/engine/decls.rs:639-642`. | Value nodes including `param`, `call_res`, `ret`, and `closure`; `std/flow.dl` joins their `fn`, file, and line fields. |
| `df_edge` | The lift appends `(from,to)` at `src/engine/extract/dataflow.rs:362-367` and writes it at `src/engine/extract/dataflow.rs:486-487`. The relation is declared at `src/engine/decls.rs:687-691`. | Intra-procedural value edges. TS call arguments are emitted as direct edges at `src/graph/typegraph/ts/flow.rs:360-398`; Rust parameters and return nodes are seeded at `src/graph/typegraph/rust/mod.rs:773-800`; Kotlin's lift is driven at `src/graph/typegraph/kotlin.rs:73-88`; Go's lift is documented at `src/graph/typegraph/go.rs:615-627`. |
| `df_param` | `param_pos` becomes `(id,pos)` at `src/engine/extract/dataflow.rs:395-400` and is written at `src/engine/extract/dataflow.rs:498-499`. The positional contract, including receiver omission, is `src/engine/decls.rs:718-726`. | Parameter node to typed parameter slot bridge. |
| `df_arg` | `f.args` becomes `(call,pos,arg,rev)` at `src/engine/extract/dataflow.rs:401-416`; the public relation is a distinct view at `src/engine/decls.rs:727-740`. TS's positional emission is visible at `src/graph/typegraph/ts/flow.rs:360-398`; Kotlin's receiver and named-argument rules are at `src/graph/typegraph/kotlin.rs:158-165`. | Call/new node to argument node, with receiver `-1`. |
| `df_field` | `f.fields` becomes rows at `src/engine/extract/dataflow.rs:418-434`; the public view and column contract are `src/engine/decls.rs:753-768`. | Field-sensitive argument flow used by `arg_field_flow`, even though the four example queries do not select it directly. |
| `call_site` | The extractor creates raw call-site rows at `src/engine/extract/call.rs:306-339`; the public family projects them at `src/engine/family/call_site.rs:1-8` and `src/engine/family/call_site.rs:28-57`. | `call_site(_,_,callee,file,line)` is re-keyed by `std/flow.dl:43-49`. |
| `call_edge` | The corpus-wide resolver builds name, SCIP, alias, and caller indexes at `src/engine/extract/call.rs:162-219`; it emits only when both caller and callee resolve at `src/engine/extract/call.rs:320-329`. The public family aggregates and projects the edge at `src/engine/family/call_edge.rs:1-14` and `src/engine/family/call_edge.rs:34-70`. | Resolved caller-symbol to callee-symbol graph, later stripped to bare symbols by `call_edge_bare`. |
| `call_name` | The declaration says it maps a def symbol to its bare callable name at `src/engine/decls.rs:614-618`. Its input is the callable-def family, so it is derived from extracted callable definitions, not a separate AST pass. | Name pin in `call_target` at `std/flow.dl:107-114`. |
| `type_sig` | Type extraction flattens each parameter and return reference at `src/engine/extract/type_rels.rs:250-264` and writes the public relation at `src/engine/extract/type_rels.rs:340-348`. The relation shape is `sym,slot,pos,ref` at `src/engine/decls.rs:530-532`. | Parameter slot type join at `examples/flow-interproc.dl:53-56`; receiver is omitted consistently with `df_param`. |
| `call_edge_bare` | No extractor produces this relation. `std/flow.dl:20-32` derives it from `call_edge` with one leading-repo strip. | Bare-symbol bridge between repo-qualified call edges and `df_node.fn`. |
| `call_site_at`, `call_node`, `call_target` | These are DL-derived joins in `std/flow.dl:40-49`, `std/flow.dl:96-114`, and do not have separate v5 extractor producers. | Per-site name pin for forward and return hops. |
| `flow_summary`, `flow_sanitizer`, `flow_lambda`, `flow_lambda_ret` | These are declared model/input relations at `std/flow.dl:51-71` and `std/flow.dl:137-157`. They are empty unless an importing program asserts facts. | Optional model and higher-order inputs. No extractor receipt exists or is required for the default empty set. |
| `flow_modeled`, `flow_kept`, `flow_cut`, `flow_edge`, `flow_lambda_call`, `arg_field_flow` | These are derived by the rules in `std/flow.dl:73-95`, `std/flow.dl:116-135`, `std/flow.dl:159-183`, and `std/flow.dl:185-205`. | The shared union and its optional higher-order/field views. |
| `closure` | This is a body operator, not a stored relation producer. The v5 parser creates `BodyItem::Closure` at `src/parse/ops.rs:431-437`; the operator is documented at `src/engine/decls.rs:241-242`. | `flow_reach(a,b) <- closure(flow_edge)` at `examples/flow-interproc.dl:40-42`. |

## 2. v6 coverage map

The extractor contract defines four families and the flat record shapes at
`v6/sprefa-extract/src/bin/extract.rs:261-301`. Its phase-1 limits leave type
references bare and omit resolved cross-file links; `--resolve PATH...` is the
separate project operation at `v6/sprefa-extract/src/bin/extract.rs:101-103` and
`v6/sprefa-extract/src/bin/extract.rs:132-195`.

| consumed rel / capability | v6 status | adapter or precision loss |
| --- | --- | --- |
| `scan` | COVERED | The file-set host is `want_at -> file` in `v6/dl/fixtures/flagship-callgraph.dl6:102-107`; the rig posts the pinned demand at `v6/tsv2/scripts/flagship-callgraph.sh:245-250`. There is no extractor `scan` record. |
| `df_node` | COVERED with seam adaptation | `record=node family=df` is specified at `v6/sprefa-extract/src/bin/extract.rs:268-278` and flattened from DfF at `v6/sprefa-extract/src/wire.rs:181-205`. The wire carries span, kind, and name, while the enclosing callable is derived at the seam, as stated at `v6/sprefa-extract/src/wire.rs:181-185`; v5's `fn` column must be reconstructed by callable-span containment. |
| `df_edge` | COVERED | `record=edge family=df kind=direct` is in the schema at `v6/sprefa-extract/src/bin/extract.rs:269-270`, and DfF flattening emits span-to-span edges at `v6/sprefa-extract/src/wire.rs:196-204`. The v6 DfF contract calls `Direct` the v5 unkinded edge at `v6/sprefa-extract/src/types.rs:536-548`. |
| `df_param` | ABSENT | DfF `Aux` is `()` and the extractor explicitly defers `args`, `fields`, `lits`, and `param_pos` at `v6/sprefa-extract/src/types.rs:553-557` and `v6/sprefa-extract/src/types.rs:1326-1330`. Add a df auxiliary record carrying the parameter node span and `pos`, with receiver/self omission matching v5. |
| `df_arg` | ABSENT | The same deferred aux contract covers argument slots. Add a record carrying the call/new node span, argument node span, and signed slot, including receiver `-1`, named-argument source slots, and closure arguments. A span-only `record=edge family=df` cannot recover the slot. |
| `df_field` | ABSENT | Add a df auxiliary record carrying composite/call node span, field text, and value node span. The required v5 shape is `id,field,value` at `src/engine/decls.rs:753-768`. |
| `call_site` | COVERED with coordinate adaptation | `record=site family=call` carries span and callee text at `v6/sprefa-extract/src/bin/extract.rs:272-287` and `v6/sprefa-extract/src/wire.rs:120-143`. The v6 seam must derive 1-based line and caller containment from the span. |
| `call_edge` | COVERED for Rust, TS, and Go project inputs; ABSENT for Kotlin in the current CLI resolver | `--resolve` emits `resolved_edge` records with caller/callee paths, names, and kind at `v6/sprefa-extract/src/bin/extract.rs:183-195` and the schema at `v6/sprefa-extract/src/bin/extract.rs:274-307`. `resolve_call_edges` dispatches only TS, Rust, Go, and Prolog at `v6/sprefa-extract/src/bin/extract.rs:199-209`; the original v5 glob includes Kotlin at `examples/flow-interproc.dl:38`, so Kotlin needs a resolver arm or a corpus exclusion. The record has no call-site span, so exact per-site pinning is lost when a caller contains multiple same-name sites. |
| `call_edge_bare` | COVERED as a derived adapter | Normalize `resolved_edge` paths/names into the v5 qualified symbol shape, then apply the same leading-repo strip as `std/flow.dl:20-32`. The v6 record has no repo column, so multi-repo identity needs an outer root/repo key. |
| `call_name` | COVERED as a derived adapter | v6 call nodes carry callable kind/name in `record=node family=call`; the record contract and flattening are `v6/sprefa-extract/src/bin/extract.rs:269-273` and `v6/sprefa-extract/src/wire.rs:120-135`. Build the symbol-to-bare-name projection after span-to-symbol normalization. |
| `type_sig` | COVERED with type-reference precision loss | `record=sig family=type` carries owner span, slot, position, and type text at `v6/sprefa-extract/src/bin/extract.rs:270-285` and `v6/sprefa-extract/src/wire.rs:84-106`. All four language columns are marked ported at `v6/sprefa-extract/src/types.rs:1315-1321`. Phase 1 leaves `ty` as a bare unresolved name at `v6/sprefa-extract/src/bin/extract.rs:303-307`; v5's `type_sig.ref` may be resolved/qualified. |
| `flow_edge` | APPROXIMABLE | Direct local edges are present. The arg-to-param and ret-to-call-site union needs `df_arg`, `df_param`, and a per-site resolved target; only the ret nodes are already present in DfF's node vocabulary at `v6/sprefa-extract/src/bin/extract.rs:291-300`. A broad all-arguments-to-all-parameters fallback would lose positional precision and is not an exact port. |
| `closure` / `flow_reach` | COVERED as recursive capability, pending full inputs | v6 supports positive single-reference self recursion in the compiler subset at `v6/prolog/compile/analyze.pl:10-16`, lowers a recursive support seed through a recursive CTE at `v6/prolog/compile/lower.pl:1159-1205`, and the tagged status records recursive strata plus the P3 cycle guard at `CLAUDE.md:637-647`. The existing callgraph fixture intentionally excludes its transitive `reaches` relation at `v6/dl/fixtures/flagship-callgraph.dl6:42-45`; flow needs its own recursive grade. |
| model rels `flow_summary`, `flow_sanitizer`, `flow_lambda`, `flow_lambda_ret` | COVERED as ordinary EDB inputs, with higher-order precision pending `df_arg` | These are program facts, not extractor families. Empty defaults can be declared in v6. Higher-order closure-slot joins require the missing argument-slot record. |

The extractor DfF status is explicit: nodes and direct edges are parity-green for
TS, Rust, Go, and Kotlin at `v6/sprefa-extract/src/types.rs:1315-1324`, while
`df` aux is deferred at `v6/sprefa-extract/src/types.rs:1326-1330`.

## 3. `closure(flow_edge)` spelling

In v5, `closure(flow_edge)` is parsed as a special `BodyItem::Closure`, not as a
call to a user relation. `src/ast.rs:441-446` documents the AST meaning, and
`src/ast.rs:568-573` recognizes the exact shape
`head(..) <- closure(edge).`. Declaration maps the head to a closure edge at
`src/engine/declare.rs:49-70`. The head becomes an SCC-backed reachability view
at `src/engine/declare.rs:1209-1244`; the evaluator loads edge rows, condenses
them, and avoids materializing the raw quadratic pair table at
`src/engine/derive.rs:2122-2129`. A pinned query uses the condensation walk at
`src/engine/query.rs:25-45`; an unpinned query falls through to full view
materialization and is guarded by edge count at `src/engine/query.rs:48-69`.

The v6 options are:

```dl6
rel flow_reach(from: text, to: text).
flow_reach(from, to) <- flow_edge(from, to).
flow_reach(from, to) <- flow_reach(from, middle), flow_edge(middle, to).
```

This is option (a), direct two-rule recursion. It uses the existing relation and
positive-rule vocabulary, with one recursive self-read. The current compiler
subset permits single-reference self recursion at `v6/prolog/compile/analyze.pl:10-16`
and emits the recursive support seed at `v6/prolog/compile/lower.pl:1186-1205`.

Option (b) is a `closure(edge)` surface form that expands to those two rules
before v6 stratification. `closure` is already a v5/SQL/Prolog graph term and is
listed as a v5 body vocabulary item at `src/engine/decls.rs:241-242`; it therefore
fits the stated vocabulary law. The expansion still needs a rule-head naming
policy, two generated variables, and restrictions for non-two-column or mixed
bodies. It also must preserve a single recursive read so that the present
compiler support applies.

Recommendation: use option (a) in the first port. It adds two ordinary rules to
the fixture and no v6 parser or lowering surface. Add option (b) only when a
shared closure spelling is required by more than this port.

| option | immediate work | correctness condition |
| --- | --- | --- |
| (a) direct recursion | Add the two rules above and a recursive fixture leg. | Exact least-fixpoint rows, cyclic convergence, and add/delete support-count behavior. |
| (b) closure sugar | Add parser expansion plus the same recursive fixture leg. | Expansion must be byte-visible in the rule graph and retain the single-self-read restriction. |

## 4. Grading rig delta

The existing callgraph rig supplies the shell and isolation pattern. It pins a
literal corpus at `v6/tsv2/scripts/flagship-callgraph.sh:112-128`, uses a scratch
root and `DL_STATE_DIR` at `v6/tsv2/scripts/flagship-callgraph.sh:28-31`, runs
the v5 source program unchanged at `v6/tsv2/scripts/flagship-callgraph.sh:210-224`,
and posts v6 arrivals at `v6/tsv2/scripts/flagship-callgraph.sh:245-250`. The
flow rig should copy the same structure, with the v5 command constrained by the
brief to:

```sh
SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1 DL_STATE_DIR="$WORK/state" \
  "$DL_V5_BIN" "$REPO/examples/flow-interproc.dl" --db "$WORK/v5.sqlite"
```

The scratch root must contain the literal mixed-language corpus under `src/...`
and `std/flow.dl` at the path used by the `use` line in
`examples/flow-interproc.dl:35`. The v6 side should run `extract --family` for
`call`, `type`, and `df`, plus `extract --resolve PATH...` for the project edge
stream. The extractor CLI emits phase-1 family lines in ordinary mode and
resolved edges only in project mode at `v6/sprefa-extract/src/bin/extract.rs:93-121`
and `v6/sprefa-extract/src/bin/extract.rs:132-195`.

Artifacts should be normalized to one semantic row identity before comparison:

| artifact bucket | normalized input | classification proof |
| --- | --- | --- |
| extracted df | `(file, callable, node span, kind, name)`, `(from span,to span)`, plus future arg/param aux | Prove v5-only/v6-only rows from the source bytes and the extractor's span records, as `flagship-classify.py` does for def/call shapes at `v6/tsv2/scripts/flagship-classify.py:77-140`. |
| extracted call/type | normalized callable symbols, call sites, resolved edges, and signature slots | Check path/name resolution, missing Kotlin resolve output, and bare type references separately. Do not collapse a missing v5 row into a v6-only explanation. |
| derived `flow_edge` | exact rule output from each engine's own normalized facts | Add a rule-fidelity leg analogous to `derive_calls` and `derive_unused` at `v6/tsv2/scripts/flagship-classify.py:143-182`; require the v5 flow rules to reproduce each engine's own `flow_edge` before assigning remaining diffs to an extraction-input bucket. |
| derived typed views | `flow_param_type` and `flow_node_type` rows keyed by normalized callee symbols and parameter spans | Rule-fidelity separately for `sink_callee`, the type-slot join, and the node-param join. `flow_node_type` cannot pass until `df_param` exists. |
| transitive output | `flow_reach` rows, or deterministic seeded cones | Classify recursive result differences only after exact `flow_edge` inputs are available and the recursive engine's add/delete behavior is checked. |

The classifier must retain the three buckets already established by the rig:
extraction-input difference, v6 expression gap, and real defect. The existing
classifier states those buckets at `v6/tsv2/scripts/flagship-classify.py:1-9`,
and its rule-fidelity requirement is `v6/tsv2/scripts/flagship-classify.py:37-53`
with failure rows emitted at `v6/tsv2/scripts/flagship-classify.py:168-182`.

`flow_reach` needs a distinct grading decision. The hermetic v5 run required by
the brief completed with 200 scanned files, 128,045 `df_node` rows, 116,777
`df_edge` rows, 6,131 `df_param` rows, 46,826 `df_arg` rows, 38,007
`call_site` rows, 8,014 `call_edge` rows, 5,862 `type_sig` rows, and 147,664
`flow_edge` rows. Its unpinned `? flow_reach(from,to).` query was refused because
the v5 closure guard saw 147,664 edge rows against the default 20,000 cap. The
guard and the reason are implemented at `src/engine/query.rs:48-69`. The exact
full closure row count was therefore not measured.

Two valid grading legs remain:

1. Grade a deterministic set of pinned forward and reverse cones using the
   seeded query paths at `src/engine/query.rs:27-45`. This avoids materializing
   the full pair relation and tests cyclic traversal without changing the
   transitive semantics of each cone.
2. Grade the complete `flow_reach` relation in a separate budgeted leg with the
   v5 closure cap explicitly disabled or raised, canonical row sorting, a wall
   clock limit, and recorded row, RSS, and database counts. The same complete
   output must be materialized from the v6 recursive stratum. The existing
   callgraph rig intentionally omits `reaches` from both the program and grade
   at `v6/tsv2/scripts/flagship-callgraph.sh:45-53`, so its four-relation success
   does not cover this leg.

| rig delta | price | dependency |
| --- | --- | --- |
| New mixed-language pinned corpus and `std/flow.dl` placement | One literal corpus list, one scratch-root copy block, one file-set assertion | None beyond the existing callgraph rig. |
| v5 flow execution and four query artifact dumps | Four normalized SQLite query dumps, plus the required hermetic env and `DL_STATE_DIR` assertion | v5 program must run from the scratch root with its imported std file. |
| v6 extractor adapter | One family decoder for spans, one project-edge decoder, and symbol/line normalization | `df` direct records and `resolved_edge`; Kotlin resolver remains a gap. |
| Per-bucket classifier | One classifier extension for raw df/call/type rows and one rule-fidelity implementation for `flow_edge` and typed views | Exact normalized input sets. |
| Closure grade | Either a fixed seed list and cone artifacts, or one separately budgeted full-closure run | Full `flow_edge` facts plus recursive-stratum output. |

## 5. Smallest correct port scope

The four query heads have different minimum fact requirements:

| query | today | required condition for exact v5 parity |
| --- | --- | --- |
| `flow_edge` | Partial only. Direct intra-procedural `df_edge` rows, ret nodes, call nodes, type signatures, and resolved project edges exist. | `df_arg` and `df_param` are required for positional arg-to-param hops; per-site resolved target identity is required to prevent same-name call-site merges. |
| `flow_reach` | Local-only approximation can walk today's direct df graph. The requested full relation is not gradeable today. | Exact `flow_edge`, then the two-rule recursive stratum and a closure/cone grade. |
| `flow_param_type` | Gradeable on a Rust/TS/Go corpus at function-level precision by joining resolved caller/callee names to `sig` slots. `sig.ty` is bare in v6 phase 1. The original Kotlin-inclusive glob waits on the missing Kotlin resolver arm. | Callable span-to-symbol normalization, resolved edge ingestion, and an explicit decision whether bare v6 type names are the parity target. |
| `flow_node_type` | Approximation only. The df `param` node exists, but its positional index is deferred. | `df_param(node_span,pos)` plus callable and type-sig joins. |

Ranked gaps and next-step prices:

1. **Df argument and parameter auxiliary facts, P0.** Add wire records for
   `(param span,pos)` and `(call/new span,pos,arg span)`, flatten them, ingest
   them into v6 relations, and emit them from the Rust, TS, Go, and Kotlin
   projectors. Preserve receiver `-1`, typed-parameter indexing, named-argument
   source slots, and closure values. Price: two new record shapes, four
   language projector emissions, one seam adapter, and one cross-language
   fixture. This unblocks exact `flow_edge`, `flow_reach` inputs, and
   `flow_node_type`. The deferred status and v5 target rows are
   `v6/sprefa-extract/src/types.rs:1326-1330` and
   `src/engine/extract/dataflow.rs:395-416`.

2. **Resolved-edge normalization and Kotlin dispatch, P0.** Convert
   `resolved_edge` path/name records to the v5 symbol space, retain enough
   caller-site identity for the `call_target` pin, and add the Kotlin resolver
   branch or exclude Kotlin with a declared grade boundary. Price: one project
   adapter, one occurrence-identity decision, one Kotlin resolver arm, and one
   same-name multi-site fixture. Current limitations are
   `v6/sprefa-extract/src/bin/extract.rs:164-170` and
   `v6/sprefa-extract/src/bin/extract.rs:199-209`; the v5 per-site pin is
   `std/flow.dl:96-114`.

3. **Direct recursive flow-reach spelling and tests, P1.** Add the two direct
   recursion rules, compile them through the single-self-read path, and grade
   DAG, cycle, insertion, and retraction output. Price: two rules, one small
   recursive fixture, one cyclic fixture, and one tick-log assertion. The v6
   recursive support path is `v6/prolog/compile/lower.pl:1186-1205`; cycle-guard
   behavior is recorded at `CLAUDE.md:637-647`.

4. **Flow-specific rig and classifier, P1 after facts.** Extend the proven
   callgraph shell with normalized df/call/type artifacts, rule-fidelity checks,
   and a separate closure budget. Price: one shell copy of the existing rig,
   one Python classifier extension, four raw relation dumps, four query dumps,
   and either a seed-cone list or a full-closure budget receipt. Existing
   classifier rules are at `v6/tsv2/scripts/flagship-classify.py:143-182`.

5. **Optional `df_field` enrichment, P2 for the four-query set.** Add
   `(composite span, field, value span)` if `arg_field_flow` is part of the
   shared flow std contract. Price: one auxiliary record, four projector arms,
   one seam table, and one field-sensitive fixture. It is required by
   `std/flow.dl:185-205` but not selected by the four queries in
   `examples/flow-interproc.dl:71-74`.

| dispatch state | can start immediately | waits on extraction work |
| --- | --- | --- |
| v6 program shape | `flow_param_type` on a Rust/TS/Go corpus, after symbol/span adapter definition; direct recursive rule fixture; seeded closure grade design | Kotlin-inclusive `flow_param_type` unless Kotlin resolution is added. |
| v6 fact work | Direct df node/edge and type/call family ingestion | `df_arg`, `df_param`, exact `flow_edge`, full `flow_reach`, and `flow_node_type`. |
| grading | Rig skeleton, input normalization, classifier structure, and raw direct-edge parity leg | Exact interprocedural and transitive parity. |

DISPATCH-READY? YES for the rig skeleton, direct DfF ingestion, and a scoped `flow_param_type` rail on languages with `--resolve` support. WAIT for the full four-query port on `df_arg`/`df_param` extraction, Kotlin resolved-edge coverage, and the separately budgeted transitive-closure grade.
