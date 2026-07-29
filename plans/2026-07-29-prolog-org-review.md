# V6 Prolog organization and duplication review

Date: 2026-07-29
Scope: analysis only, HEAD `15a25c08c4a6790a9cfffb045e314cf6851db4ad`

## Executive counts

| Measure | Result | Receipt |
|---|---:|---|
| `.pl` files below `v6/prolog/` | 46 | `find v6/prolog -type f -name '*.pl'` |
| Total physical lines | 15,028 | `wc -l` over the 46 files |
| Explicit named modules | 23 declarations, 22 distinct names | Module declarations listed below; `emit_ts` is declared twice at `compile/emit_ts.pl:27` and `src/emit_ts.pl:21` |
| Files without a `module/2` directive | 23 | 22 load into `user`; `plunit_tests.pl` creates nine test modules |
| Explicit local import/load SCCs with more than one node | 0 | Import graph below |
| Private qualified call sites | 9 sites, 7 distinct predicates | SWI `library(check)` plus source receipts below |
| Shipping traversal, evaluation, and structural rewrite definitions | 18 | Body-walker inventory below; 17 are body-specific and `compile_value_terms/2` is a whole-term rewrite |
| Older `src/` body traversal definitions | 2 | `src/checks.pl:38-39`, `src/emit_ts.pl:102-128` |
| Mirrored program checks with equivalent trigger classes | 6 | Refusal matrix below |
| Raw static unused-export candidates | 40 | `prolog_xref` over all 46 files |
| Strong unused-export candidates after exception review | 12 exports | Unused-export table below |
| Undefined predicates from loaded-code checks | 0 | SWI 10.0.2 `list_undefined/1` over compile, test, and example clusters |
| Current conformance | 135 pass, 0 fail | Executed `swipl -q -l v6/prolog/conformance/go.pl -g go -g halt` |
| Current PLUnit | 74 pass, 0 fail | Executed `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g run_tests -g halt` |
| Current saved sweep artifacts | 73 compiled, 62 unsupported; 70 identical, 0 wrong, 2 run errors, 1 no oracle log | `v6/prolog/compile/out/manifest.json`, `v6/prolog/compile/out/run-results.json`; the writer is `v6/tsv2/scripts/sweep.sh:1-32` |

## 1. Complete file inventory and module boundaries

`user` means no `module/2` directive. The fixture files contribute
`user:fixture/5`; the loader and declaration are at
`conformance/go.pl:14-24` and `conformance/engine.pl:75-77`. The PLUnit file
creates nine test modules at `compile/test/plunit_tests.pl:66-994`.

| Area | File | Lines | Module boundary |
|---|---|---:|---|
| root | `v6/prolog/0_enum_expand.pl` | 145 | `enum_expand` at `:12` |
| root | `v6/prolog/0_match_expand.pl` | 149 | `match_expand` at `:10` |
| root | `v6/prolog/1_host_expand.pl` | 442 | `host_expand` at `:11-17` |
| root | `v6/prolog/ARCH.pl` | 541 | `user`; self-checking fact database, entry commands at `:3-4` |
| compile | `v6/prolog/compile/1_emit_registry_docs.pl` | 223 | `emit_registry_docs` at `:7-10` |
| compile | `v6/prolog/compile/analyze.pl` | 947 | `analyze` at `:18-29` |
| compile | `v6/prolog/compile/compile.pl` | 161 | `compile` at `:26-33` |
| compile | `v6/prolog/compile/emit_ts.pl` | 1,158 | `emit_ts` at `:27` |
| compile | `v6/prolog/compile/lower.pl` | 1,584 | `lower` at `:127` |
| compile | `v6/prolog/compile/oracle_dump.pl` | 90 | `user`; absence of module is deliberate at `:18-20` |
| compile | `v6/prolog/compile/parse_dl.pl` | 1,098 | `parse_dl` at `:51` |
| compile | `v6/prolog/compile/print_dl.pl` | 462 | `print_dl` at `:21-23` |
| compile | `v6/prolog/compile/registry.pl` | 97 | `registry` at `:10-16` |
| compile | `v6/prolog/compile/scripts/text_door_receipt.pl` | 250 | `text_door_receipt` at `:78` |
| compile | `v6/prolog/compile/strat.pl` | 138 | `strat` at `:28` |
| compile | `v6/prolog/compile/sweep.pl` | 235 | `sweep` at `:22` |
| compile test | `v6/prolog/compile/test/plunit_tests.pl` | 994 | Nine `plunit_*` modules at `:66-994` |
| compile test | `v6/prolog/compile/test/run_sql_check.pl` | 416 | `run_sql_check` at `:32` |
| conformance | `v6/prolog/conformance/body.pl` | 139 | `body` at `:6-8` |
| conformance | `v6/prolog/conformance/engine.pl` | 463 | `engine` at `:58-60`; reexports `body:json_canon/2` at `:61` |
| conformance | `v6/prolog/conformance/go.pl` | 29 | `user` |
| conformance | `v6/prolog/conformance/level_eval.pl` | 213 | `level_eval` at `:6-7` |
| conformance | `v6/prolog/conformance/rulings.pl` | 355 | `rulings` at `:8` |
| conformance | `v6/prolog/conformance/ticklog.pl` | 168 | `user` |
| fixture | `v6/prolog/conformance/fixtures/0_enum_variants.pl` | 52 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/1_match_block.pl` | 150 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/2_hosts_wiring.pl` | 242 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/check_eventing.pl` | 272 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/engine_core.pl` | 193 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/expressions.pl` | 280 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/json_arm.pl` | 182 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/merge_family.pl` | 158 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/occurrence_identity.pl` | 269 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/operators.pl` | 57 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/scopes.pl` | 413 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/shell_stream.pl` | 169 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/spine_semantics.pl` | 302 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/state_machine.pl` | 95 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/temporal_pipe.pl` | 341 | `user:fixture/5` facts |
| fixture | `v6/prolog/conformance/fixtures/timeless_rail.pl` | 632 | `user:fixture/5` facts |
| example | `v6/prolog/examples/ghcacher.pl` | 113 | `user` |
| older source | `v6/prolog/src/checks.pl` | 44 | `checks` at `:6-9` |
| older source | `v6/prolog/src/emit_ts.pl` | 249 | `emit_ts` at `:21`, collides with compile emitter |
| older source | `v6/prolog/src/grader.pl` | 16 | `grader` at `:4` |
| older source | `v6/prolog/src/kernel.pl` | 52 | `kernel` at `:5-6` |
| tool | `v6/prolog/tools/arch_map.pl` | 250 | `user` |
| **total** | **46 files** | **15,028** | **23 declarations, 22 distinct names** |

### Local import and load graph

Library imports are omitted. Each edge is backed by the displayed
`use_module/1,2`, `ensure_loaded/1`, or `reexport/2` receipt.

| Consumer | Local dependencies |
|---|---|
| `match_expand` | `enum_expand` (`0_match_expand.pl:13`) |
| `host_expand` | `registry` (`1_host_expand.pl:20`) |
| `body` | none |
| `level_eval` | `body` (`conformance/level_eval.pl:12`) |
| `engine` | `match_expand`, `host_expand`, `rulings`, `body`, `level_eval` (`conformance/engine.pl:67-71`); reexport of one `body` predicate at `:61` |
| `go` in `user` | `grader`, `engine` (`conformance/go.pl:11-12`); fixtures loaded at `:14-24` |
| `ticklog` in `user` | `go`, `body` (`conformance/ticklog.pl:31-32`) |
| `oracle_dump` in `user` | `ticklog` (`compile/oracle_dump.pl:24`) |
| `registry` | none |
| `analyze` | `match_expand`, `body`, `registry` (`compile/analyze.pl:34-39`) |
| `strat` | `analyze` (`compile/strat.pl:33-34`) |
| `lower` | `analyze`, `body` (`compile/lower.pl:131-132`) |
| compile `emit_ts` | `lower`, `analyze`, `host_expand` (`compile/emit_ts.pl:36-38`) |
| `parse_dl` | `registry` (`compile/parse_dl.pl:57`) |
| `print_dl` | `analyze`, `registry` (`compile/print_dl.pl:27-31`) |
| `compile` | `match_expand`, `host_expand`, `analyze`, `strat`, `lower`, compile `emit_ts`, `parse_dl` (`compile/compile.pl:36-42`) |
| `sweep` | `compile`, `lower`, compile `emit_ts`, `body` (`compile/sweep.pl:27-30`) |
| `run_sql_check` | `compile`, `lower` (`compile/test/run_sql_check.pl:39-40`) |
| PLUnit modules | `compile`, `strat`, `lower`, `analyze`, `enum_expand`, `match_expand`, `parse_dl`, `print_dl`, `host_expand`, compile `emit_ts` (`compile/test/plunit_tests.pl:16-27`) |
| `text_door_receipt` | `compile`, `print_dl` (`compile/scripts/text_door_receipt.pl:82-84`) |
| `emit_registry_docs` | `registry` (`compile/1_emit_registry_docs.pl:12`) |
| `checks` | `kernel` (`src/checks.pl:12`) |
| older source `emit_ts` | `grader` (`src/emit_ts.pl:28`) |
| `ghcacher` in `user` | `kernel`, `checks`, `grader` (`examples/ghcacher.pl:19-21`) |
| `ARCH` in `user` | `kernel`, `rulings` (`ARCH.pl:246`, `:289`) |
| `arch_map` in `user` | `ARCH`, `rulings` (`tools/arch_map.pl:13-14`) |

### Cycles, diamonds, and namespace collision

| Finding | Data |
|---|---|
| Circular imports | No explicit local import/load SCC contains more than one file. |
| Compile diamond | `compile` imports `analyze`, `lower`, and `emit_ts`; `emit_ts` imports both `lower` and `analyze`; `lower` also imports `analyze` (`compile/compile.pl:38-41`, `compile/emit_ts.pl:36-37`, `compile/lower.pl:131`). This is a DAG with repeated direct edges. |
| Conformance diamond | `engine` imports `body` directly and through `level_eval` (`conformance/engine.pl:70-71`, `conformance/level_eval.pl:12`). |
| Expansion redundant edge | `compile:program_plan/2` expands at `compile/compile.pl:92-96`, then `analyze:check_supported_subset/1` expands again at `compile/analyze.pl:737-739`. |
| Namespace collision | Loading `compile/emit_ts.pl` and `src/emit_ts.pl` in one SWI process fails with `No permission to redefine module emit_ts`; declarations are at `compile/emit_ts.pl:27` and `src/emit_ts.pl:21`. |

### Qualified calls into private predicates

SWI `library(check)` reported these with `module_class([user,test])`.
Exported qualified calls such as `strat:sql_rule_order/2` at
`plunit_tests.pl:107`, `engine:fixture_expectations_hold/2` at
`conformance/go.pl:26`, and `emit_ts:emit_program/5` at
`compile/sweep.pl:119` are excluded.

| Caller receipt | Private target |
|---|---|
| `compile/test/plunit_tests.pl:294` | `lower:canonical_column_expr/2` |
| `compile/test/plunit_tests.pl:359` | `emit_ts:incremental_program_safe/4` |
| `compile/test/plunit_tests.pl:365` | `emit_ts:incremental_program_safe/4` |
| `compile/test/plunit_tests.pl:366` | `emit_ts:reconcile_every_tick/2` |
| `compile/test/plunit_tests.pl:374` | `emit_ts:derived_edge_carry_required/3` |
| `compile/test/plunit_tests.pl:380` | `emit_ts:derived_edge_carry_required/3` |
| `compile/test/plunit_tests.pl:407` | `lower:level_support_sql/4` |
| `compile/test/plunit_tests.pl:415` | `emit_ts:retraction_guard/2` |
| `examples/ghcacher.pl:109` | `checks:body_member/2` |

## 2. Repeated body and conjunction algorithms

### Shipping walkers

| Walker | Descent semantics | Registry use | Consolidation and drift risk |
|---|---|---|---|
| `body:solve/2` (`conformance/body.pl:96-110`) | Evaluates comma left to right; descends into `not/1`; wrapper meanings are executable | No; 11 forms are clauses | Keep as evaluator. A structural walker cannot preserve binding order, backtracking, and negation execution by itself. |
| `body:body_atoms/2` (`conformance/body.pl:112-126`) | Flattens comma; treats `not`, `latest`, `finalize`, `pre`, `now`, binds, guards, and JSON forms as leaves with zero atoms | No; hardcoded form list | Can become a projection over a shared walker. Risk: descending into `not/1` would change trigger candidates. |
| `body:substitute_goal/3` (`conformance/body.pl:129-133`) | Descends only comma; replaces one term by identity; wrappers remain opaque | No | Keep policy-specific. Risk: descending into a wrapper could remove a sampled or negated goal that is not the firing occurrence. |
| `engine:trigger_items_/2` (`conformance/engine.pl:146-164`) | Flattens comma; `finalize` becomes departure; every bare atom becomes arrival; all other special forms, including `not`, are opaque | No; hardcoded form list | Shared traversal can supply node roles, while trigger projection remains local. Risk: `finalize` is an occurrence source although the registry currently marks it refused for compiler lowering at `registry.pl:22`. |
| `engine:body_finalize_ref/2` (`conformance/engine.pl:183-185`) | Descends comma, does not descend `not` | No | Consolidate with policy `descend_not(false)`. Risk: a generic relation-ref projection would include bare atoms. |
| `engine:body_latest_ref/2` (`conformance/engine.pl:187-190`) | Descends comma and `not`; returns only `latest` refs | No | Direct shared-walker candidate. |
| `engine:body_pre_ref/2` (`conformance/engine.pl:192-195`) | Descends comma and `not`; returns only `pre` refs | No | Direct shared-walker candidate. |
| `level_eval:goal_rel_refs/3` (`conformance/level_eval.pl:102-119`) | Flattens comma; descends `not` and moves all inner refs to negative; `latest` is positive; ignores `finalize`, `pre`, guards, binds, and JSON forms | No; hardcoded form list | Can project from the registry roles already used by `body_ref_uses/2`. Risk: `latest` is a positive dependency here while it remains marked `sampled`; aggregate rules add a strict gap later at `level_eval.pl:92-100`. |
| `analyze:body_ref_uses/2` (`compile/analyze.pl:95-122`) | Flattens comma; registry drives wrappers, `not`, `next`, and variadic `combine`; retains sign and trigger versus sampled marking | Yes, through `body_surface_for_term/6` at `:98-100` | Existing base for a shared structural walker. Drift risk is low if output order and `flip_to_neg/2` behavior remain byte-equivalent. |
| `analyze:conjunction_goals/2` (`compile/analyze.pl:649-656`) | Flattens comma, variadic `combine`, and `next`; leaves `not` opaque | Partial hardcode: `combine` and `next` | Can use shared traversal with splice roles. Risk: callers rely on `not(Inner)` remaining one goal, including `negated_guard_goal/2` at `:847-852`. |
| `analyze:level_body_latest_ref/2` (`compile/analyze.pl:762-765`) | Descends comma and `not`; returns only `latest` refs | No | Same semantics as `engine:body_latest_ref/2`; direct consolidation. |
| `analyze:level_body_pre_ref/2` (`compile/analyze.pl:767-770`) | Descends comma and `not`; returns only `pre` refs | No | Same semantics as `engine:body_pre_ref/2`; direct consolidation. |
| `analyze:reserved_construct_in_body/2` (`compile/analyze.pl:783-797`) | Descends comma only; checks registry status at each reached leaf | Yes | Shared walker candidate. Current drift: reserved forms inside `combine(...)` are not reached although `body_ref_uses/2` splices `combine` via `registry.pl:24`. |
| `analyze:negated_guard_goal/2` (`compile/analyze.pl:847-852`) | Flattens outer conjunction, selects a `not` leaf, then flattens the inner conjunction without further `not` descent | Yes indirectly through `body_guard_goals/2` | Keep projection, reuse traversal. Risk: nested negation depth changes which guard is refused. |
| `analyze:body_forbidden_goal/2` (`compile/analyze.pl:927-947`) | Descends comma only; checks registry refusal role at a leaf | Yes | Shared walker candidate. Current drift: it does not splice `combine` and does not descend `not`. The latter is partly covered by the separate negated-guard check. |
| `host_expand:compile_value_terms/2` (`1_host_expand.pl:62-76`) | Recurses through every compound argument and compiles `ts_query/1` values anywhere | No body classification | Keep as a general term rewrite. A body walker would narrow its current whole-term reach. |
| `host_expand:body_goals/2` (`1_host_expand.pl:315-320`) | Flattens comma, leaves every wrapper opaque | No | Consolidate only the comma flattening. Probe selection at `:297-312` depends on wrappers remaining whole. |
| `print_dl:print_body/3` (`compile/print_dl.pl:336-343`) | Prints left item as a leaf and recurses down the right comma spine | Per-item registry dispatch at `:358-380` | Keep rendering policy but consume a shared ordered goal list. Risk: a left-nested conjunction currently reaches generic term printing, while parser output is right-associated at `parse_dl.pl:743-755`. |

### Expansion and older-source sibling walkers

These traverse adjacent syntax shapes rather than the shipping body semantics,
but they duplicate the same flatten or recurse mechanics.

| Walker | Shape and receipt | Consolidation |
|---|---|---|
| `enum_expand:enum_variant/2` | Flattens semicolon variants at `0_enum_expand.pl:83-88` | Keep enum-specific projection; a generic binary-tree flatten helper could be shared, with little line benefit. |
| `match_expand:match_arms/2` | Flattens semicolon arms at `0_match_expand.pl:71-76` | Same mechanical shape as enum variants. |
| `match_expand:enum_variant/2` | Repeats the enum semicolon walker at `0_match_expand.pl:134-139` | Direct duplication of `enum_expand:enum_variant/2`; sharing enum metadata closes this site. |
| `src/checks:body_member/2` | Flattens comma and treats everything else as a candidate at `src/checks.pl:38-39` | Older-source cluster; also reached privately from `examples/ghcacher.pl:109`. |
| `src/emit_ts:body_list/2`, `item_holes/3`, `item_names/5` | Right-spine flatten at `src/emit_ts.pl:102-103`; hardcoded negation/comparison cases at `:111-128` | Older-source cluster with a separate AST (`\+/1`, `cmp_op/4`). Do not merge with the shipping registry walker without an AST adapter. |

### One registry-driven walker: priced boundary

Proposed signature for evaluation:

```prolog
walk_body(+Body, +WalkPolicy, -Events).

% event(Path, Polarity, SurfaceRole, Term)
% WalkPolicy declares:
%   conjunction forms, splice forms, whether not/1 descends,
%   whether a wrapper is emitted before or after its children.
```

Traversal order remains left to right. The walker holds no dynamic state and
allocates only the result list and path/context terms. Registry lookup happens
once per reached compound node. Consumers project events into refs, trigger
items, goals, or violations.

| Consolidation class | Sites | Result |
|---|---|---|
| Direct, same observable semantics | Engine and analyzer `latest` walkers; engine and analyzer `pre` walkers; level dependency refs via registry roles; reserved and forbidden scans; host comma flatten; printer ordered goal list | 9 definitions can share traversal mechanics. |
| Shared traversal, local projection required | `body_atoms`, `trigger_items_`, `body_ref_uses`, `conjunction_goals`, negated guards | 5 definitions retain small policy/projection predicates. |
| Remain separate | `solve/2`, `substitute_goal/3`, `compile_value_terms/2`, parser construction, enum/match semicolon expansion, older `src/` AST | Binding, rewrite, or different-AST behavior would drift. |

Estimated change size is 45 to 65 lines for the walker and policies, deleting
70 to 105 lines from the 14 eligible definitions, for an estimated net
reduction of 20 to 50 lines. A characterization test must first cover
comma association, nested `not`, `next`, variadic `combine`, `latest`,
`finalize`, `pre`, a reserved lifecycle form, a bind, a comparison, and a
plain relation atom.

## 3. Mirrored cross-plane refusals

### Equivalent trigger classes

| Check | Engine implementation | Compiler implementation | Trigger or diagnostic drift |
|---|---|---|---|
| Log declaration on level-headed relation | `engine.pl:111-112` using `level_headed/2` at `:96` | `analyze.pl:744-746` using `rule_is_level/1` and `rule_head_ref/2` | Trigger class is equivalent. Diagnostics differ only by outer `unsupported_construct/1`. Covered on both sides at `fixtures/engine_core.pl:59-66` and `plunit_tests.pl:534-539`. |
| `latest/1` in level rule | `engine.pl:119-120`, walker at `:187-190` | `analyze.pl:747-749`, walker at `:762-765` | Walkers both descend comma and `not`; equivalent. Covered at `engine_core.pl:68-75` and `plunit_tests.pl:541-544`. |
| `pre/1` in level rule | `engine.pl:121-122`, walker at `:192-195` | `analyze.pl:750-752`, walker at `:767-770` | Walkers both descend comma and `not`; equivalent. Covered at `engine_core.pl:77-84` and `plunit_tests.pl:546-549`. |
| Keyed level head | `engine.pl:107-108` | `analyze.pl:756-758` | Equivalent existential condition. Covered at `fixtures/1_match_block.pl:114-127` and `plunit_tests.pl:803-808`. |
| Keyed Log relation | `engine.pl:109-110` | `analyze.pl:759-760` | Trigger equivalent. Engine throws `keyed_log_rel(Ref)`; compiler throws `unsupported_construct(keyed_log_rel(Ref, Positions))`. The compiler has no direct PLUnit receipt. Engine coverage is `engine_core.pl:24-29`. |
| `finalize/1` in level rule | Specific engine check at `engine.pl:117-118`, shallow walker at `:183-185` | Generic refused-goal path through `check_level_rule_shape/1` at `analyze.pl:827-839` and `body_forbidden_goal/2` at `:927-947` | Both catch a direct or comma-contained `finalize`; both leave `not(finalize(...))` opaque. Diagnostics drift: engine names `finalize_in_level_rule(Ref)`; compiler names `level_body_goal(Head, finalize(Atom))`. No direct compiler PLUnit receipt. |

### Checks present on one side

| Side | Check family | Receipt | Drift consequence |
|---|---|---|---|
| Engine only | Missing retention on Log | `engine.pl:113-114`; fixture `engine_core.pl:17-22` | The compiler can accept a Log declaration with no `keep/2` even though the reference door rejects it. This is the loudest one-sided program-shape check. |
| Engine only | Aggregate in edge head | `engine.pl:115-116` | The compiler edge check at `analyze.pl:772-781` checks body shape and head arithmetic, but no aggregate-edge condition. A compound aggregate argument can reach generic head-expression lowering. Needs a test before any shared-check extraction. |
| Engine runtime only | Edge write into unkeyed Set | Runtime throw at `engine.pl:274-284`; fixture `engine_core.pl:31-38` | This is outside `check_program/1`, but it is another cross-door acceptance difference if compiler lowering admits the shape. |
| Engine runtime only | Retraction from Log | `engine.pl:214-218`; fixture `engine_core.pl:40-45` | Schedule-sensitive rather than a static `prog/2` check. It does not belong in a program-only shared module unless schedule validation is added. |
| Compiler only | Reserved `zip/2` and lifecycle arms | `analyze.pl:741-743`, `:783-797`; registry rows `registry.pl:25`, `:27-30` | Reference execution has no matching named static check. |
| Compiler only | Edge body subset, including finalize, pre, now, negation, binds, comparisons, JSON destructuring | `analyze.pl:590-647`, enforced at `:753-754` | These are compiler capability refusals. The reference engine intentionally executes a wider language. They should remain compiler-only policies. |
| Compiler only | Same-key edge conflict risk | `analyze.pl:799-825` | Reference engine checks actual occurrence outputs at runtime at `engine.pl:252-265`; compiler refuses the static risk class because its generated arms cannot reproduce that check. |
| Compiler only | Head arithmetic refusal and typed comparison constraints | `analyze.pl:772-781`, `:916-925`; lowering checks at `lower.pl:461-491` | Compiler representation limits, not shared language invalidity. |
| Compiler only | Aggregate lowering limits | `analyze.pl:827-900` | JSON aggregates, self-read, and no-positive-body checks are compiler capability boundaries. Reference aggregate behavior is in `level_eval.pl:22-33`, `:92-100`, `:165-190`. |
| Already shared before either door | `bind_and_rule_head` | `host_expand.pl:183-193`; both doors call `prepare_program/5` at `engine.pl:379-382` and `compile.pl:92-95` | This is the working one-file, two-consumer shape requested by the task. |

### Shared check module price

Proposed signature:

```prolog
program_violation(+Program, +CheckName, -Payload).
check_program_common(+Program, +OrderedChecks, -Violation).
```

One numbered root file, placed after its dependencies, can own common
declaration/rule predicates and the six equivalent trigger classes. Engine and
compiler adapters retain their current exception envelopes and check ordering.
Compiler capability refusals remain in `analyze.pl`.

The placement follows the existing shared-root precedent: both doors import
`match_expand` and `host_expand` from one implementation
(`engine.pl:67-68`, `compile.pl:36-37`), and `match_expand` itself imports the
single enum implementation (`0_match_expand.pl:13`).

| Price axis | Estimate |
|---|---|
| Common checks closed | 6 mirrored trigger classes |
| One-sided holes exposed for tests | 2 engine-only static checks: missing retention and aggregate edge head |
| New module | 65 to 90 lines including ordered check facts and violation payloads |
| Deleted duplicated code | 45 to 65 lines across `engine.pl` and `analyze.pl`, including duplicate latest/pre walkers |
| Net lines | Roughly +10 to +30 initially |
| Main risk | Error order and exact exception terms are observable fixture data |
| Required new tests | Compiler missing-retention parity, compiler aggregate-edge parity, compiler finalize diagnostic, compiler keyed-Log payload decision, nested-`not` parity |

The line count is initially positive because adapters preserve two error
vocabularies. The measurable benefit is six checks with one trigger
implementation and two current holes made explicit.

## 4. Expansion pipeline

### Current pipeline

The current APIs do not all expose `expand_*_program/2`:

| File | Actual API | Receipt |
|---|---|---|
| `0_enum_expand.pl` | `expand_enum_program/2` | `:12`, implementation `:19-23` |
| `0_match_expand.pl` | `expand_match_program/2` | `:10`, implementation `:18-20` |
| `1_host_expand.pl` | `prepare_program/5` plus host/query helpers | `:11-17`, implementation `:26-46` |

The two main callers manually execute:

```text
prepare_program
  -> expand_match_program
       -> expand_match_rules
       -> expand_enum_program
```

Receipts are `compile/compile.pl:92-96`,
`conformance/engine.pl:379-382`, and `0_match_expand.pl:18-20`.
`analyze:check_supported_subset/1` performs match plus enum expansion again at
`compile/analyze.pl:737-739`.

### Forced future order and current incompatibility

The spreading lab records `enum -> declaration spread -> row spread -> match`
at `plans/2026-07-29-rel-spreading-verdict.md:173-175` and again at
`:430-432`. No spread expander is present in the 46-file inventory because the
lab code was removed after the verdict.

A call-order edit alone would lose match exhaustiveness:

1. Match coverage reads `enum_decl/2` from the unexpanded declarations at
   `0_match_expand.pl:98-112`.
2. Enum expansion removes each `enum_decl/2`, replacing it with ordinary
   `col_type/3`, `keyed/2`, and tag rules at `0_enum_expand.pl:65-81`.
3. Running enum before the current match expander therefore makes
   `enum_variant_refs/3` at `0_match_expand.pl:123-132` see no enum metadata.

The driver needs an expansion context carrying enum name, variants, and
generated refs, or match coverage must derive that map from the expanded
declarations and generated tag rules.

### Declared-order driver price

Proposed signatures:

```prolog
expansion_phase(10, enum,        enum_expand:expand_enum_program).
expansion_phase(20, decl_spread, spread_expand:expand_decl_spread_program).
expansion_phase(30, row_spread,  spread_expand:expand_row_spread_program).
expansion_phase(40, match,       match_expand:expand_match_program).

expand_program(+SurfaceProgram, -ExpandedProgram, -ExpansionContext).
```

Host preparation either remains a pre-pass or is split into normalization,
world-plan extraction, and probe expansion. Its current combined lifecycle is
`1_host_expand.pl:26-46`, so placing all of it casually inside the four-phase
table would mix syntax normalization with metadata extraction.

| Option | Order sites | Estimated code size | Drift exposure |
|---|---:|---:|---|
| Status quo after spread lands | At least engine, compile, analyzer gate, tests, and match's hidden enum call | About 2 calls per consumer plus phase-specific glue | Every consumer can omit or reorder a phase; analyzer already double-expands |
| One driver with order facts | One order table and one fold; consumers call one predicate | 25 to 40 new lines, 10 to 20 deleted now; each future consumer saves 3 to 5 calls | One order implementation; requires enum metadata repair for match coverage |

This proposal needs a new test first. The test must assert both expanded term
identity and the `match_nonexhaustive/2` refusal after enum-first expansion.
Current enum and match tests at `plunit_tests.pl:708-848` do not run the future
four-phase order.

## 5. Registry reach and remaining hardcoded functor families

`registry.pl:1-8` states the intended parser, printer, analyzer, and gate
inventory. Current reach is:

| Consumer | Registry-driven part | Hardcoded duplicate or adjacent list | Drift site |
|---|---|---|---|
| Analyzer body refs | Full body role dispatch through `body_surface_for_term/6` (`analyze.pl:95-115`) | None for registered body roles | Existing reference implementation for a shared walker. |
| Parser body items | Wrapper and word inventory from `surface/5` (`parse_dl.pl:762-777`) | Prefix `!rel` and ordinary relation fallback at `:778-786`; these are grammar forms rather than registered call functors | Low for registered wrappers. |
| Printer body items | Registry role dispatch (`print_dl.pl:358-380`) | Arithmetic operators and precedence remain five facts at `:402-419` | Arithmetic is duplicated in analyzer and lowerer, but has no registry rows. |
| `body.pl` | None | Six comparison clauses at `:45-46`; eleven body special cases at `:96-126` | Adding a guard or wrapper row can update parser/analyzer while reference execution and `body_atoms/2` remain unchanged. |
| `level_eval.pl` | None | Aggregate list `[count,sum,min,max,json_array]` plus `json_object/2` at `:28-33`; body form list at `:102-119` | Aggregate status can change in registry without oracle aggregate classification changing. `json_array` is recognized by oracle while refused by compiler at `registry.pl:67`. |
| `lower.pl` | Analyzer supplies body roles | Arithmetic list at `:378-380`; ordered and identity comparison lists at `:476-489` | Comparison functors duplicate registry rows `registry.pl:49-54`; arithmetic duplicates analyzer and printer but has no registry rows. |
| `analyze.pl` expression/type paths | Aggregate recognition mostly uses `surface_for_term/6` at `:456-463`, `:882-900` | `[sum,min,max]` at `:461`; arithmetic lists at `:535-537` and `:921-925` | Registry says which aggregate is live, while type behavior still has a local subset. |
| `0_match_expand.pl` | None | 22 `match_language_form/1` clauses at `:48-69` | Duplicates binds, comparisons, wrappers, `match`, and `true`; also lists `departed/1`, which has no registry row. A new surface row can be misclassified as a positive relation source. |
| `engine.pl` | None | Trigger form clauses at `:149-164`; latest/pre/finalize scans at `:183-195` | Same surface additions can become accidental trigger atoms. |

Two registry extensions have different risk:

1. Moving registered body-form classification into a shared walker closes the
   body, level, engine, and analyzer drift sites without changing expression
   metadata.
2. Adding arithmetic metadata such as precedence, result type, and SQL
   operator closes the five-operator duplication in
   `analyze.pl:535-537`, `lower.pl:378-380`, and
   `print_dl.pl:419`. This widens `surface/5` beyond body syntax and should use
   a separate `expression/5` table or an explicit axis contract.

## 6. SWI Prolog analyzing this Prolog

### Tools and executed findings

| Tool | Exact capability here | Executed finding |
|---|---|---|
| [`library(prolog_xref)`](https://www.swi-prolog.org/pldoc/man?section=prologxref) | Parses source without loading it; provides definitions, exports, calls, modules, and source lines through `xref_source/2`, `xref_called/5`, `xref_defined/3`, and `xref_exported/2` | All 46 files cross-referenced with exit 0. It found 40 exports with no cross-file static import/call. Meta-calls and `ensure_loaded/1` make that a candidate list, not a deletion list. |
| [`library(prolog_codewalk)`](https://www.swi-prolog.org/pldoc/doc/_SWI_/library/prolog_codewalk.pl?public_only=false) | Walks loaded clauses, meta-calls, multifile predicates, and initializations; can trace references with source positions | Used indirectly by `library(check)`. It resolves autoloads and meta-predicates better than raw source xref. |
| [`library(check)`](https://www.swi-prolog.org/pldoc/man?section=summary-lib-check) | Reports undefined predicates, private cross-module calls, redefinitions, trivial fails, bad format calls, and other loaded-code checks | `list_undefined([module_class([user,test])])` reported 0 undefined predicates in compile/test and example clusters. Cross-module checking reported the 9 private call sites above. |
| `predicate_property/2` and `module_property/2` | Runtime reflection for `exported`, `imported_from`, `file`, `line_count`, `number_of_clauses`, module exports, and module class | Confirmed compile modules are class `user`; PLUnit modules are class `test`, which must be included explicitly in checks. |
| [`gxref/0`](https://www.swi-prolog.org/pldoc/man?section=prologxref) | XPCE graphical front end over `prolog_xref` | Useful for interactive call-graph browsing; unsuitable as the CI gate because it requires a GUI. |

Additional real findings:

| Finding | Receipt |
|---|---|
| Duplicate module | Loading both emitter files fails immediately because each declares `emit_ts`: `compile/emit_ts.pl:27`, `src/emit_ts.pl:21`. |
| Private API reach | 9 sites and 7 predicates listed in section 1. |
| Raw unused-export candidates | 40 exports: 2 host, 2 registry-doc CLI, 2 SQL-check CLI, 2 printer, 4 compile CLI/API, 2 sweep/text-door CLI, 9 analyzer, 2 kernel, 3 older emitter, 5 checks, 1 body, 6 engine. |
| Strong unused-export candidates | `host_expand:compile_query/2` and `host_relation_refs/3` are only called inside their defining module (`1_host_expand.pl:40`, `:95`, `:177-207`); analyzer exports `decl_keep/3`, `arrival_target_refs/2`, `edge_headed_refs/2`, `level_headed_refs/2`, `rel_columns/4`, both `rel_column_types` arities, `snake_name/2`, and `conjunction_goals/2` despite no external source call (`analyze.pl:18-29`, definitions at `:54-57`, `:73-79`, `:196-199`, `:216-218`, `:252-256`, `:287-296`, `:649-656`); `body:body_atoms/2` has no caller in the 46 files (`body.pl:7`, `:112-126`). |
| Xref exceptions | CLI entry predicates are invoked with `-g`, including registry docs (`1_emit_registry_docs.pl:3-5`), compile APIs (`compile.pl:13`), sweep (`sweep.pl:20`), and older emitter APIs. `ARCH` and the conformance chain use `ensure_loaded/1`, which raw unused-export analysis does not fully attribute (`tools/arch_map.pl:13`, `ticklog.pl:31`, `oracle_dump.pl:24`). |

### `just prolog-lint` gate recipe

A repository gate should load clusters separately because the duplicate
`emit_ts` module currently prevents an all-files load.

```text
prolog-lint:
  1. xref all 46 source files; fail on parse errors.
  2. group xref_module/2 results; fail on duplicate module names.
  3. load compile/test/plunit_tests.pl.
  4. run list_undefined with module classes [user,test].
  5. run cross-module-call checking with module classes [user,test].
  6. run list_redefined, list_void_declarations, list_trivial_fails,
     and list_format_errors.
  7. start a fresh SWI process, load examples/ghcacher.pl, and repeat
     undefined and cross-module checks.
  8. emit the unused-export candidate table as advisory output.
```

`check/0` alone uses default module classes and misses PLUnit class `test`.
The gate wrapper should capture SWI warning terms and exit nonzero for
undefined, duplicate-module, private-cross-module, redefinition, void
declaration, trivial-fail, and format findings. Unused exports remain advisory
because CLI entry points and meta-calls require classification.

### Aside: how good Prolog is at analyzing Prolog here

SWI handles syntax, module resolution, loaded meta-calls, multifile fixtures,
source locations, and private qualified calls with little custom code:
`prolog_xref` parsed all 46 files, loaded-code checks produced zero undefined
predicates, and the private-call scan returned exact lines for 9 sites. It also
found the `emit_ts` module collision immediately. Its weak point here is
application entry reach: 40 exports look unused to source xref, while several
are invoked through `-g`, `ensure_loaded/1`, or meta wrappers such as
`quiet/1` in `text_door_receipt.pl:155-198`. The useful split is automated
failure for undefined/private/module consistency, with unused exports emitted
for human classification.

## 7. Ranked refactor table

No proposal is executed in this arc. Line figures are estimates based on the
source spans cited above.

| Rank | Proposal | Benefit: drift sites closed and lines | Risk and protecting gates | Test-first status |
|---:|---|---|---|---|
| 1 | Shared registry-driven body walker with policy projections | Closes 9 direct traversal sites and supplies mechanics for 5 more; delete 70 to 105 lines, add 45 to 65, estimated net reduction 20 to 50 | High: negation, trigger marking, `finalize`, and left-to-right goal order. Protect with conformance 135, PLUnit 74, sweep 73/70/0, roundtrip, and TEXT_DOOR. Receipts: `body.pl:96-133`, `engine.pl:146-195`, `level_eval.pl:102-119`, `analyze.pl:95-122`, `:649-656`, `:762-770`, `:783-797`, `:927-947`. | **NEEDS NEW TEST FIRST** for nested wrappers, `combine`, `next`, and comma association. |
| 2 | Shared cross-plane program-check module with per-door error adapters | One trigger implementation for 6 mirrored checks; exposes 2 engine-only static holes; delete 45 to 65 lines, add 65 to 90, estimated initial net addition 10 to 30 | High: exception order and payloads are fixture-visible. Protect with conformance refusal fixtures and PLUnit supported-subset tests, then sweep and TEXT_DOOR. | **NEEDS NEW TEST FIRST** for missing retention, aggregate edge head, finalize error, keyed-Log payload, and nested `not`. |
| 3 | Single expansion driver with declared phase order and expansion context | Closes at least 4 order sites and the analyzer's double expansion; 25 to 40 new lines, 10 to 20 deleted now; future spread insertion becomes one order row | High: enum-first currently erases metadata needed by match coverage (`0_enum_expand.pl:65-81`, `0_match_expand.pl:98-132`). Protect with enum/match PLUnit, conformance enum/match fixtures, roundtrip, sweep, and TEXT_DOOR. | **NEEDS NEW TEST FIRST** for enum-first exhaustive and nonexhaustive match. |
| 4 | Move reference aggregate and body classification onto registry roles | Closes `level_eval` aggregate list and 10 body-form clauses plus corresponding `body.pl` classification sites; estimated 20 to 35 deleted, 10 to 20 added | Medium-high: oracle semantics must remain wider than compiler status, especially live oracle `json_array` versus compiler refusal (`level_eval.pl:28-33`, `registry.pl:67`). Protect with conformance aggregate and JSON fixtures, sweep final-state leg, PLUnit aggregate gate. | Existing tests cover common cases; add a registry-row-to-oracle-classification table test first. |
| 5 | Add expression metadata table for five arithmetic and six comparison operators | Closes 5 local lists across body/analyze/lower/print and gives parser/printer/type/lowering one operator inventory; estimated 20 to 35 deleted, 20 to 30 added | Medium: precedence, integer division, modulo correction, and comparison type rules differ by consumer (`body.pl:29-53`, `lower.pl:378-414`, `:461-494`, `print_dl.pl:402-422`). Protect with conformance expression fixtures, PLUnit expression snapshots, roundtrip, sweep, TEXT_DOOR. | Existing coverage is broad; add an inventory-totality test for every operator row. |
| 6 | Rename or isolate older `src/emit_ts` module and add module-uniqueness lint | Closes 1 hard all-load failure; about 1 rename plus import/caller edits, and 5 to 10 lint lines | Medium because external `-g emit` use may name the module implicitly. Protect with `ARCH.pl` self-check, example checks, old emitter invocation, and new lint. Existing main battery does not load both emitters. | **NEEDS NEW TEST FIRST** because existing gates cannot detect the collision. |
| 7 | Reduce exported API to externally reached predicates, with explicit CLI declarations | Closes 13 strong export drift sites; declaration-only deletion of about 13 indicators, with possible dead-code follow-up for `decl_keep/3`, `arrival_target_refs/2`, `rel_column_types/5,7`, and `body_atoms/2` | Medium: xref has CLI/meta-call false positives. Protect with xref advisory report, command receipts, conformance, PLUnit, and roundtrip. | Classify each candidate first; CLI entry points stay exported or get documented `public/1`. |
| 8 | Replace 9 private qualified test/example calls with public behavioral seams or explicit test exports | Closes 9 reach sites across 7 private predicates; line change depends on whether tests move to public output snapshots | Medium: these tests currently pin emitter/lower internals. PLUnit protects behavior, while `library(check)` protects the boundary. | Existing PLUnit assertions protect current internals; add equivalent public-output snapshots before removing each reach. |
| 9 | Share declaration facts `rel_kind`, `decl_key`, and headed-ref queries before shared checks | Closes duplicated relation-kind logic at `engine.pl:89-98` and `analyze.pl:45-57`; estimated 10 to 18 deleted, 12 to 20 added | Medium: engine `rel_kind/4` accepts an unused Rules argument while analyzer exposes `rel_kind/3`; fallback-to-set behavior must stay exact. Protect with conformance kind/key fixtures, compiler DDL snapshots, sweep. | Existing kind/key fixtures cover common cases; add direct parity table tests. |
| 10 | Add `just prolog-lint` as a read-only gate | Detects undefined predicates, private calls, duplicate modules, redefinitions, void declarations, trivial fails, and format errors; about 25 to 45 lines for a runner and recipe | Low runtime risk, medium rollout work because current tree has 9 private sites and one duplicate module. It is itself the protection for organization changes. | Gate initially reports a checked-in baseline; fail-new can land before the existing findings are removed. |

## 8. Run record under the single-artifact fence

| Command or evidence | Result | Write posture |
|---|---|---|
| `git rev-parse HEAD` | Exact required hash | Read-only |
| `prolog_xref` over all 46 files | Exit 0 | Read-only |
| Loaded compile/test `list_undefined`, private-call check, and `list_redefined` | 0 undefined, 8 private call sites, 0 predicate redefinitions | Read-only |
| Loaded example cluster checks | 0 undefined, 1 private call site | Read-only |
| Load both emitter files | Expected failure on duplicate module `emit_ts` | Read-only |
| Conformance runner | 135 pass, 0 fail | Read-only |
| PLUnit runner | 74 pass, 0 fail | Read-only |
| Saved sweep JSON | 73 compiled, 70 identical, 0 wrong, 3 rejection-path results | Read-only inspection; sweep was not rerun because its driver writes `compile/out` and `tsv2/gen_emitted` at `v6/tsv2/scripts/sweep.sh:1-32` |
| Roundtrip | Not rerun; it regenerates `compile/dl_view` at `compile/scripts/roundtrip.sh:112-156` | Would violate the one-output-file instruction |
| TEXT_DOOR | Not rerun; it creates `compile/out/text-door` and generated `.ts`/`.dl6` files at `text_door_receipt.pl:93-99`, `:153-200`, `:235` | Would violate the one-output-file instruction |

## 9. Top 10 findings

1. The tree contains 46 Prolog files and 15,028 physical lines; 7 files
   outside the named conformance/compile/shared minimum are still active
   inventory: `ARCH.pl`, `examples/`, `src/`, and `tools/` entries in section
   1.
2. There are no circular local imports, but both
   `compile/emit_ts.pl:27` and `src/emit_ts.pl:21` declare module `emit_ts`;
   an all-files load fails.
3. The shipping paths contain 18 traversal, evaluation, and structural
   rewrite definitions, of which 17 are body-specific.
   Nine can share traversal mechanics directly, and five more can share the
   walk while retaining local projections.
4. `analyze:body_ref_uses/2` at `analyze.pl:95-122` already supplies the
   registry-driven semantics needed for the shared structural walker,
   including `not`, `next`, and variadic `combine`.
5. Six cross-plane invalid-program trigger classes are duplicated between
   `engine:check_program/1` at `engine.pl:106-122` and compiler analysis at
   `analyze.pl:741-770`, with diagnostic drift for keyed Log and finalize.
6. Missing Log retention and aggregate edge heads are checked only by the
   engine (`engine.pl:113-116`). They are the two loudest static acceptance
   differences before extracting a shared check module.
7. Both main doors run host then match then enum
   (`compile.pl:92-96`, `engine.pl:379-382`,
   `0_match_expand.pl:18-20`), while the spread verdict requires enum, decl
   spread, row spread, then match
   (`plans/2026-07-29-rel-spreading-verdict.md:173-175`).
8. Reordering the current expanders would silently disable match
   exhaustiveness because match reads `enum_decl/2` at
   `0_match_expand.pl:98-132` and enum expansion removes it at
   `0_enum_expand.pl:65-81`; a driver needs expansion metadata.
9. Registry reach is partial: parser, printer body items, and analyzer body
   refs use it, while body execution, level dependencies, engine triggers,
   match source classification, arithmetic, and comparison lowering retain
   local functor lists (`body.pl:45-46`, `level_eval.pl:28-33`,
   `engine.pl:149-164`, `0_match_expand.pl:48-69`,
   `lower.pl:378-489`).
10. SWI 10.0.2 is sufficient for a repository lint gate: it reported zero
    undefined predicates in loaded clusters, exact receipts for 9 private
    qualified calls, 40 raw unused-export candidates, and the duplicate
    module failure. Unused exports need CLI and meta-call classification;
    undefined, private-call, and module-uniqueness findings can fail
    automatically.
