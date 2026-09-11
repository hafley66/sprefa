# DL7 `int_lt/2` kernel migration

Date: 2026-09-10  
Base: `8c0b0f693f272d878ef9f948a81694b36078ac09`

## Ownership

| Area | Files |
| --- | --- |
| Kernel registry, slots, keys | `src/2_comptime/0_lowerer.pl` |
| Kernel schema, type rows, mode checking | `src/2_comptime/1_checker.pl` |
| Native positive and negative evaluation | `src/1_libtime/0_evaluator.pl` |
| Removal of generated predecessor seeds | `src/2_comptime/2_compiler.pl`, `src/1_libtime/0a_syntax_macro_program.pl` |
| Six ordering consumers | `prelude/3_derived_rules.dl7`, `prelude/4_type_algebra.dl7` |
| Removed `before/3` declaration and rules | `prelude/1_declarations.dl7`, `prelude/3_derived_rules.dl7` |
| Macrotime ordering | `macrotime/0_standard.dl7`, `test/fixtures/14_syntax_macros.dl7` |
| SQLite scalar lowering | `src/3_emit/1c_sqlite_query_emitter.pl` |
| Exact behavior and performance coverage | `test/1_entrypoints.test.pl`, `test/1a_syntax_expander.test.pl`, `test/9_dbsp_plan.test.pl`, `test/14_sqlite_query_emitter.test.pl`, `test/20_compiler_performance.test.pl`, `bench/0_compiler_performance.pl` |

## Kernel contract

The registry and checked graph expose `int_lt/2` with slots `left:int` and
`right:int`, key `[[0,1]]`, positive mode `(+,+)`, and semidet native
evaluation. Positive underbound goals produce exactly
`underconstrained_kernel_goal(int_lt, [[0,1]])`. A grounded negative goal uses
the logical complement of the same integer comparison. Constants of another
type produce `kernel_argument_type_mismatch/4`.

No reader or infix syntax was added. No other arithmetic kernel was added.

## Ordering migration and removal audit

The migrated prelude calls are:

| Consumer | Native comparison |
| --- | --- |
| `key_predecessor/3` | `int_lt(PriorIndex, SourceIndex)` |
| `pick_predecessor/4` | `int_lt(PriorIndex, SourceIndex)` |
| `exclude_predecessor/4` | `int_lt(PriorIndex, SourceIndex)` |
| `curry_predecessor/3` | `int_lt(PriorIndex, SourceIndex)` |
| left-side `intersection_predecessor/6` | `int_lt(PriorIndex, SourceIndex)` |
| right-side `intersection_predecessor/6` | `int_lt(PriorIndex, SourceIndex)` |

After migration, an executable-file search over `v7/src`, `v7/prelude`,
`v7/macrotime`, and shipped fixtures found zero calls to `predecessor/3` or
`before/3`. The remaining words such as `curry_predecessor` are userland helper
relation names and do not call the removed kernel relation. The registry,
schema rows, keys, generated checked seeds, frozen-round seeds, macrotime
seeds, declaration, and closure rules were then removed.

## Compiler output

Fixture: `test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Both captures
contain unique whole rows only and empty diagnostics.

| Relation | Baseline | Migrated | Delta |
| --- | ---: | ---: | ---: |
| `before` | 13,364 | 0 | -13,364 |
| `kernel(predecessor)` | 424 | 0 | -424 |
| `kernel(:)` | 525 | 519 | -6 |
| `kernel(node)` | 136 | 135 | -1 |
| `kernel(product)` | 132 | 131 | -1 |
| `kernel(module)` | 2 | 2 | 0 |
| `closed_names` | 1 | 1 | 0 |
| `Option` | 1 | 1 | 0 |
| `kernel(nil)` | 1 | 1 | 0 |
| total | 14,586 | 790 | -13,796 |

The 13,796-row count delta consists of the 13,364 `before` rows, 424
predecessor seeds, and the net eight removed ordering-metadata rows. Runtime
inventory changed from 128 relations, 122 rules, and 424 seeds to 127
relations, 120 rules, and zero seeds.

The raw whole-row set comparison removed 14,397 rows and added 601. Its raw
relation split was:

```text
removed: before 13364, predecessor 424, colon 437, node 86, product 86
added:   colon 431, node 85, product 85
```

Deleting the prelude declaration renumbers later `reader_node(prelude, N)`
identities. After isolating the removed `before` identity, predecessor kernel
identity, and added `int_lt` identity, every residual changed row contains a
prelude reader identity: 599 removed graph rows and 597 added graph rows, all
limited to `kernel(:)`, `kernel(node)`, and `kernel(product)`. No other
executable relation call changed. The semantic relation distribution above
isolates the net eight ordering-metadata rows from that identity churn.

Baseline top fanouts were `before` over the fixture module at 6,670,
`before` over the prelude module at 6,328, fixture `kernel(:)` at 116,
fixture predecessor at 115, prelude `kernel(:)` at 113, and prelude
predecessor at 112. Migrated top fanouts are fixture `kernel(:)` at 115,
prelude `kernel(:)` at 112, five owner-specific `kernel(:)` groups at 6,
then owner-specific groups at 5.

## Performance

The cold nearest-shadow gate ran with trace collection off. Its 785 ms result
is below three seconds, so no traced profile was run.

| Measure | Baseline | Migrated | Delta |
| --- | ---: | ---: | ---: |
| cold wall | 2,931 ms | 785 ms | -2,146 ms |
| cold inferences | 14,581,429 | 6,369,740 | -8,211,689 |
| warm wall | 74 ms | 4 ms | -70 ms |
| warm inferences | 2,148 | 2,148 | 0 |
| compiler rows | 14,586 | 790 | -13,796 |

Profile tag: `v7-compiler-perf/nearest-shadow-int-lt@2026-09-10`.

## Backend parity

Logical-program reification preserves positive `int_lt` calls exactly. The
SQLite query emitter lowers positive calls to native `<` predicates and
grounded negative calls to native `>=` predicates, including scalar-only
rules.

The DBSP plan emitter has no scalar-predicate representation. Its reachable
path currently reports exactly
`hidden_runtime_relation(rule_id(0), kernel(int_lt))`. Supporting the call
would require adding an `int_lt` field or predicate variant to the plan IR in
`v7/src/3_emit/1a_dbsp_plan_emitter.pl` and to the shipped V6 consumer type
`Predicate` plus evaluator in `v6/dd-runner/src/kernel.rs`. V6 edits were
explicitly excluded from this change, so the existing boundary remains and is
covered by an exact reification and diagnostic test.

## Validation

All commands were bounded by `timeout 20`; the performance profile was bounded
by `timeout 15`.

- Five focused kernel and ordering tests passed: true, false, grounded negative
  true and false, positive underbound diagnostic, non-integer rejection, exact
  registry/schema/key inventory, and exact Key/Pick/Exclude/Curry/intersection
  ranks.
- All 16 test 18 cases passed in four named groups.
- All 10 test 19 cases passed in four named groups.
- All 4 syntax-expander cases and the focused module macro case passed. The
  macro slice changed from 75 to 73 rules, exactly the two removed closure
  rules.
- All 8 DBSP plan tests passed, including exact `int_lt` reification and the
  named backend boundary.
- Five SQLite query tests passed, including exact positive and negative scalar
  SQL. The native SQLite IVM lifecycle case was not run because its existing
  native library is absent and building the excluded native plugin was outside
  scope.
- All 3 compiler-tracer tests and both evaluator trace tests passed.
- All 17 compiler-performance tests and the cold nearest-shadow gate passed.
- `git diff --check` passed.

CI coverage is changed through exact tests in existing suites. No CI workflow
or test runner configuration changed.
