# DL7 grounded integer comparison family

Date: 2026-09-10
Base: `32d4de862b34f9d756f455fd38eaa6dd10880b97`

## Kernel contract

The kernel exposes this closed family:

| Predicate | Positive | Grounded negative |
| --- | --- | --- |
| `int_lt(+Left,+Right)` | `Left < Right` | `Left >= Right` |
| `int_le(+Left,+Right)` | `Left <= Right` | `Left > Right` |
| `int_eq(+Left,+Right)` | `Left = Right` | `Left != Right` |
| `int_ne(+Left,+Right)` | `Left != Right` | `Left = Right` |
| `int_ge(+Left,+Right)` | `Left >= Right` | `Left < Right` |
| `int_gt(+Left,+Right)` | `Left > Right` | `Left <= Right` |

All six predicates are arity 2, integer-only, grounded before execution,
semidet, and accepted under grounded negation. The evaluator owns one ordered
registry containing each name and its positive and complementary operators.
The lowerer, checker, evaluator, and SQLite emitter consume that registry.

Every relation has key `[[0,1]]` and slots `left:int` and `right:int`.
Positive underbound calls report
`underconstrained_kernel_goal(Name, [[0,1]])`. A non-integer argument reports
`kernel_argument_type_mismatch(Name, Position, int, Argument)`.

The existing `int_lt` behavior remains pinned by its original exact closure,
mode-diagnostic, ordering-rank, SQLite SQL, and SQLite error assertions. No
reader or infix syntax was added.

## Checked graph and compiler-row delta

The five added relations contribute only the approved kernel metadata:

| Row kind | Per relation | Added relations | Delta |
| --- | ---: | ---: | ---: |
| `node(kernel(Name))` and `product(kernel(Name))` | 2 | 5 | +10 |
| typed `left` and `right` colon edges | 2 | 5 | +10 |
| compiler rows | 4 | 5 | **+20** |

Measured checkpoints:

| Fixture/checkpoint | Base | Family | Delta |
| --- | ---: | ---: | ---: |
| nearest-shadow compiler rows | 790 | 810 | +20 |
| Partial compiler rows | 890 | 910 | +20 |
| nearest-shadow runtime relations | 127 | 132 | +5 |
| nearest-shadow runtime rules | 120 | 120 | 0 |
| nearest-shadow runtime seeds | 0 | 0 | 0 |

The checked runtime graph also gains ten node/product rows, ten slot edges,
five relation declarations, and five corresponding stratum rows. Performance
row checkpoints move from 790 to 810 and from 15,542 to 15,562. No inference
budget or wall-clock limit moved.

## SQLite lowering

Scalar lowering emits `<`, `<=`, `=`, `!=`, `>=`, and `>` for positive calls,
with `>=`, `>`, `!=`, `=`, `<`, and `<=` respectively for grounded negative
calls. This representation is private to the SQLite emitter. No shared or DBSP
IR schema changed. The existing `int_lt` SQL and error payloads remain exact.

## Performance

Profile tag:
`v7-compiler-perf/nearest-shadow-int-comparisons@2026-09-10`.

The final trace-off nearest-shadow gate reported:

| Measure | Result | Gate |
| --- | ---: | ---: |
| cold wall | 823 ms | <= 3,000 ms |
| cold inferences | 6,388,944 | <= 16,000,000 |
| warm wall | 4 ms | informational |
| warm inferences | 2,148 | <= 5,000 |
| compiler rows | 810 | exact |

An earlier loaded-system run crossed the wall gate. The required diagnostic
probe ran with `DL7_TRACE=steps` under a 15-second timeout. The subsequent
trace-off gate passed at 823 ms. The exact `int_lt` ordering-rank test passed
alone in 2.569 seconds.

## Validation

Every command was bounded by `timeout 20`; tracing probes used the stricter
15-second bound.

- Five focused evaluator, checker, and exact registry/slot tests passed. They
  cover true and false positive evaluation, true and false grounded negative
  evaluation, positive underconstraint, non-integer arguments, and all six
  predicates. Each case reported at or below 0.005 seconds.
- The original exact `int_lt` evaluator and checker tests passed unchanged.
- The `int_lt` ordering-rank test passed in 2.569 seconds.
- The SQLite family test passed in 0.004 seconds and pins all twelve positive
  and negative SQL forms plus the original constant SQL and error payloads.
- All 17 compiler-performance tests passed.
- All 4 syntax-expander tests, all 8 DBSP-plan tests, all 16 binding-symmetry
  tests, and all 10 lexical-binding tests passed. The cold first
  binding-symmetry case was rerun alone and passed in 2.680 seconds.
- The nearest-shadow performance gate passed with empty diagnostics and exact
  cold/warm output parity.
- `git diff --check` passed before commit.

## CI coverage

Added coverage: family-wide evaluator truth tables, grounded complements,
checker diagnostics, exact registry/operator/slot inventory, and twelve SQLite
operator forms. Changed coverage: kernel/compiler-row inventory and performance
checkpoints now include the five added metadata entries. Removed coverage: none.

No workflow or test-runner configuration changed.
