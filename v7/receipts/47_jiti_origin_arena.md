# 47. JITI checker origin arena

Date: 2026-09-11. Implementation worktree based on
`2702f95f8d9ea7b1a43d388cd30cc1ca7f32c139`. SWI-Prolog 10.0.2
arm64-darwin. This receipt covers only checker origin collection. The language,
kernel, binding rules, checker results, compiler rows, and phase terms are
unchanged. The evaluator stratum arena from receipt 46 remains intact.

## Pre-edit inventory

The origin list has six variants:

```prolog
origin(edge(Owner, Name, Index), NodeId)
origin(seed(SeedIndex), NodeId)
origin(rule(RuleIndex), NodeId)
origin(goal(RuleIndex, GoalIndex), NodeId)
origin(node(Owner), NodeId)
origin(relation(Target), NodeId)
```

Only the first four are checker keyed lookups. Their observed modes are fully
ground keys with one output and deterministic first-list-match behavior:

```prolog
edge_origin(+Origins, +Owner, +Name, +Index, -NodeId) is det.
seed_origin(+Origins, +SeedIndex, -NodeId) is det.
rule_origin(+Origins, +RuleIndex, -NodeId) is det.
goal_origin(+Origins, +RuleIndex, +GoalIndex, -NodeId) is det.
```

Every missing key returns `none`. `node/1`, `relation/1`, module-lowering
origin lookups, and the public origin lists remain list-backed.

The 12 checker call sites are:

| Accessor | Callers |
| --- | --- |
| `edge_origin/5` | `duplicate_bind_diagnostics/4`, `duplicate_index_diagnostics/4`, `dense_index_diagnostics_indexed/4`, `dense_index_diagnostics_scanned/4`, `resolve_edges/6` |
| `seed_origin/3` | `resolve_seeds/8` |
| `rule_origin/3` | `aggregate_cycle_origin/4`, `resolve_rules/8`, `head_safety_diagnostics_with_variables/5` |
| `goal_origin/4` | `cycle_origin/4`, `resolve_goals/9`, `mode_failure_diagnostics/4` |

A fresh nearest-shadow profile observed four `check_datalog/4` calls. Each
owning compile checks its origin list twice, initially and after final lowering:

| Owning compile | Checker calls | Origin rows | Variant counts | Duplicate keys |
| --- | ---: | ---: | --- | --- |
| macrotime | 2 | 98 | edge 37, goal 29, rule 12, node 10, relation 10, seed 0 | 0 |
| main source | 2 | 1,124 | edge 484, goal 292, rule 120, node 114, relation 114, seed 0 | 2 |

The main list duplicates `rule(0)` and `goal(0,0)`. In each case the prelude
node precedes the source-file node:

```text
rule(0):   reader_node(prelude, 980), then reader_node(nearest-shadow, 0)
goal(0,0): reader_node(prelude, 986), then reader_node(nearest-shadow, 3)
```

Source-list order is therefore observable when diagnostics request either key.

## Signatures and physical representation

```prolog
open_checker_origin_arena(+ModuleOrigins) is det.
close_checker_origin_arena is det.
with_checker_origin_lookup(+Origins, :Goal) is det.

arena_edge_origin(?Owner, ?Name, ?Index, ?ArenaId, ?Sequence, ?NodeId)
arena_seed_origin(?SeedIndex, ?ArenaId, ?Sequence, ?NodeId)
arena_rule_origin(?RuleIndex, ?ArenaId, ?Sequence, ?NodeId)
arena_goal_origin(?RuleIndex, ?GoalIndex, ?ArenaId, ?Sequence, ?NodeId)
```

The four `arena_*` predicates are dynamic flattened facts. An explicit
`dl7_checker_origin_arena_*` id is stored in every clause. Installation walks
the owning compile's local `ModuleOrigins` in module order and origin-list order
and uses `assertz/1`. A global keyed-origin sequence preserves order across the
four predicate families and detects extra, missing, reordered, or changed
checker provenance through exact unification.

`compile_units_traced/3` and `compile_project_units/5` open the arena after
successful local lowering and project installation. One
`setup_call_cleanup/3` then owns the initial check, compiler rounds, final
lowering, and final check. The initial and final nearest-shadow sequences match,
so both checker calls use the same arena. The compile does not rebuild the
store inside `check_datalog/4`.

The final origin sequence can differ for generated callables. One observed
case maps `goal(1,0)` to reader node 56 during deferred lowering and reader node
54 during final lowering. `check_datalog/4` compares its current keyed sequence
to the arena once. A matching sequence selects the JITI facts. A differing
sequence selects the original list accessors for that checker call. This guard
preserves exact final diagnostic provenance without rebuilding the arena.

`checker_origin_arena_scope/1` and `checker_origin_lookup_scope/1` are
thread-local innermost-first stacks. Cleanup pops the active id and retracts
only clauses carrying that id. Partial installation failure, goal failure, and
exceptions use the same cleanup path. Nested compiles retain the outer id;
simultaneous threads use separate ids and scopes.

## Realized JITI shape

A bounded fresh process lowered nearest-shadow, opened its main 1,124-row
origin arena, queried every present checker key, inspected
`predicate_property/2` and `library(prolog_jiti):jiti_list/1`, then cleaned up.
The sequence indexes serve exact compatibility validation; the key indexes
serve the lookup leaves.

| Predicate | Clauses | Realized key index | Buckets | Speedup | Collisions | Realized sequence index |
| --- | ---: | --- | ---: | ---: | ---: | --- |
| `arena_edge_origin/6` | 484 | arguments `2+3` | 512 | 122.6 | 64 | argument 5, 512 buckets, speedup 484.0, 58 collisions |
| `arena_rule_origin/4` | 120 | argument `1` | 128 | 109.1 | 4 | argument 3, 128 buckets, speedup 120.0, 39 collisions |
| `arena_goal_origin/5` | 292 | arguments `1+2` | 512 | 275.0 | 60 | argument 4, 512 buckets, speedup 292.0, 0 collisions |
| `arena_seed_origin/4` | 0 | none on nearest-shadow | 0 | n/a | 0 | none |

The stored full fields are unified after bucket selection. Hash collisions
cannot return a different key or node.

## Exact output parity

The fixture is
`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Before and after were
serialized with `write_canonical/1` over
`output(CompilerRows, RuntimeProgram, Diagnostics)` in this worktree and hashed
with SHA-256.

| State | Compiler rows | Runtime relations/rules/seeds | Diagnostics | SHA-256 |
| --- | ---: | --- | ---: | --- |
| before | 810 | 132 / 120 / 0 | 0 | `a2b62f13652279f7688c9421ac61326031bcdfb239cce419aa1dea869b05c36f` |
| after | 810 | 132 / 120 / 0 | 0 | `a2b62f13652279f7688c9421ac61326031bcdfb239cce419aa1dea869b05c36f` |

The existing rows-only hash in `1_entrypoints.test.pl` was left unchanged. It
was captured in the main checkout and includes absolute reader paths, so a
different worktree path produces a different rows-only hash. The new parity
test compares list-backed and arena-backed checked terms in the same process.

## Cold before and after

Each row is one fresh `swipl` process, run sequentially under `timeout 15`.
Tracing was off during measurement and the compiler caches were cleared before
the cold compile. No install or build command was run.

| State | Sample | Wall ms | Charged inferences |
| --- | ---: | ---: | ---: |
| before | 1 | 422 | 1,899,975 |
| before | 2 | 411 | 1,899,975 |
| before | 3 | 397 | 1,899,975 |
| before | 4 | 394 | 1,899,975 |
| after | 1 | 500 | 1,909,640 |
| after | 2 | 354 | 1,909,640 |
| after | 3 | 334 | 1,909,640 |
| after | 4 | 340 | 1,909,640 |

Median wall changed from 404 ms to 347 ms, a decrease of 57 ms or 14.1%.
Charged inferences changed by `+9,665`, or `+0.51%`. A separate final gate run
reported 347 ms and 1,909,640 cold inferences, then 4 ms and 2,240 warm
inferences.

The performance methodology kept setup and measurement separated: source
inventory and JITI realization ran outside the timed compiler process, and the
cold samples used the existing deterministic compiler gate.

## Targeted work counters and cleanup

The origin-list cell metric is the same full-list work counter used by receipt
43: sum `length(Origins)` over every `edge_origin/5` call.

| Counter | Before | After | Delta |
| --- | ---: | ---: | ---: |
| owning compile arena builds | 0 | 2 | +2 |
| edge-origin lookups | 4,166 | 4,166 | 0 |
| edge-origin lookups served by JITI | 0 | 4,166 | +4,166 |
| edge-origin list fallbacks | 4,166 | 0 | -4,166 |
| origin list cells | 4,378,888 | 0 | -4,378,888 |

After a complete compile, residue was:

```text
arena_edge_origin clauses = 0
arena_seed_origin clauses = 0
arena_rule_origin clauses = 0
arena_goal_origin clauses = 0
checker_origin_arena_scope rows = 0
checker_origin_lookup_scope rows = 0
```

The source remains because the full-compiler median wall decreased 14.1% and
the targeted list-cell counter decreased 100%. The deterministic charged
inference increase is 0.51%; emitted counts, lookup count, residue, and output
hash are unchanged. This receipt treats 0.51% as below material regression for
this gate.

## Tests and CI coverage

Nine focused cases in `v7/test/1_entrypoints.test.pl` passed individually under
`timeout 3`. The maximum PLUnit case time was 0.233 seconds:

- duplicate first-match across edge, seed, rule, and goal keys;
- missing-key `none` fallback;
- changed final provenance list fallback;
- lifecycle success;
- cleanup after goal failure and exception;
- partial-open cleanup;
- nested compile isolation;
- simultaneous-thread isolation;
- nearest-shadow list/arena checked-term parity.

The existing generated-callable final-freeze case passed in 0.460 seconds. The
sampled stratum lifecycle, nested-isolation, and simultaneous-isolation cases
passed at 0.002 seconds each. The nearest-shadow compiler performance gate
passed cold and warm output parity and budgets. `git diff --check` passed.

Repository test coverage adds nine cases. CI workflow coverage adds, changes,
and removes zero cases; workflow files are unchanged. The existing compiler
performance gate executes the changed path.

Changed files:

- `v7/src/2_comptime/1_checker.pl`
- `v7/src/2_comptime/2_compiler.pl`
- `v7/test/1_entrypoints.test.pl`
- this receipt

No commit, push, merge, parser edit, evaluator edit, broader collection edit,
or public phase-term edit was made.
