# DL7 grounded integer comparison family

Date: 2026-09-10
Base: `32d4de862b34f9d756f455fd38eaa6dd10880b97`

## Kernel contract

The checked kernel graph exposes these six arity-two predicates in registry
order:

```text
int_lt(+Left,+Right)
int_le(+Left,+Right)
int_eq(+Left,+Right)
int_ne(+Left,+Right)
int_ge(+Left,+Right)
int_gt(+Left,+Right)
```

Each predicate has slots `left:int` and `right:int`, key `[[0,1]]`, positive
mode `(+,+)`, and semidet evaluation. Positive calls must be grounded before
execution. Grounded negative calls use the logical complement from the same
registry row. Non-integer constants produce
`kernel_argument_type_mismatch(Name, Position, int, Argument)`.

The existing `int_lt/2` compatibility predicate and its `<` behavior remain
unchanged. No reader syntax, infix syntax, generic equality, unification
semantics, or non-integer comparison was added.

## Registry and consumers

`src/1_libtime/00_integer_comparison.pl` is the shared table-driven registry.
Each row contains the positive scalar operator, its grounded-negation
complement, and the accepted `compare/3` orderings. The lowerer, checker,
evaluator, and SQLite emitter dispatch through that registry.

The evaluator uses one grounded comparison path in ordinary rule evaluation,
grounded negation, and aggregate proof evaluation. The SQLite emitter uses one
scalar comparison IR form and one operator-token table. The emitted pairs are:

| DL7 predicate | Positive SQL | Grounded negative SQL |
| --- | --- | --- |
| `int_lt` | `<` | `>=` |
| `int_le` | `<=` | `>` |
| `int_eq` | `=` | `!=` |
| `int_ne` | `!=` | `=` |
| `int_ge` | `>=` | `<` |
| `int_gt` | `>` | `<=` |

The existing `unsupported_sqlite_int_lt_argument/2` diagnostic spelling is
retained for `int_lt`.

## Compiler row delta

The nearest-shadow fixture changed only by the approved kernel metadata:

| Relation | Base | Family | Delta |
| --- | ---: | ---: | ---: |
| `kernel(:)` | 519 | 529 | +10 |
| `kernel(node)` | 135 | 140 | +5 |
| `kernel(product)` | 131 | 136 | +5 |
| all other rows | 5 | 5 | 0 |
| total | 790 | 810 | +20 |

The ten colon rows are the two typed slots for each of the five added
relations. The five node rows and five product rows name their checked kernel
graph products. Runtime relation inventory moved from 127 to 132; rules remain
120 and seeds remain zero.

The `2_partial.dl7` exact entrypoint snapshot moved from 890 to 910 rows by the
same +20 metadata. Its runtime graph moved from 280 to 290 nodes, 532 to 542
edges, 134 to 139 relations, and 134 to 139 strata. Its seed, rule, and
dependency counts remain 1, 127, and 222. The separate historical performance
checkpoint moved from 15,542 to 15,562 by the authorized exact +20 delta.

## Ordering

The existing rank test passes without expected-value changes for Key, Pick,
Exclude, Curry, and both sides of intersection. The six prelude ordering call
sites remain `int_lt/2`; the other comparison predicates add no ordering rows.

## Performance

Profile tag:
`v7-compiler-perf/nearest-shadow-int-comparisons@2026-09-10`.

| Measure | Result | Budget |
| --- | ---: | ---: |
| cold wall | 1,322 ms | 3,000 ms |
| cold inferences | 6,390,561 | 16,000,000 |
| warm wall | 16 ms | reported only |
| warm inferences | 2,148 | 5,000 |
| compiler rows | 810 | 810 |

The focused required test slice completed with a maximum reported case time of
2.047 seconds. The nearest-shadow gate stayed below three seconds, so no traced
profile was needed for the passing final run.

## Validation

Every command was bounded with `timeout 20`.

- Seven focused entrypoint cases passed: legacy `int_lt` evaluation and mode
  behavior, all six positive and grounded-negative truth tables, all six
  underconstrained and non-integer diagnostics, exact registry/slot inventory,
  unchanged ordering ranks, and the updated compiler snapshot.
- Three focused SQLite cases passed, including exact positive and logical
  complement SQL for all six predicates.
- All 17 compiler-performance checkpoint cases passed.
- The cold nearest-shadow performance gate passed with the measurements above.
- All 8 DBSP plan cases passed; the existing scalar-predicate backend boundary
  remains unchanged.
- All 10 lexical-binding cases passed.
- `git show --check --oneline HEAD` passed on the committed diff.

CI coverage changes through focused cases in the existing entrypoint, SQLite,
and compiler-performance suites. No workflow or test-runner configuration was
changed.

## Files

| Area | Files |
| --- | --- |
| Shared registry | `src/1_libtime/00_integer_comparison.pl` |
| Native evaluation | `src/1_libtime/0_evaluator.pl` |
| Lowerer registry, slots, keys | `src/2_comptime/0_lowerer.pl` |
| Checked graph, modes, argument types | `src/2_comptime/1_checker.pl` |
| SQLite scalar lowering | `src/3_emit/1c_sqlite_query_emitter.pl` |
| Focused behavior | `test/1_entrypoints.test.pl`, `test/14_sqlite_query_emitter.test.pl` |
| Performance checkpoints | `bench/0_compiler_performance.pl`, `test/20_compiler_performance.test.pl`, `justfile` |
