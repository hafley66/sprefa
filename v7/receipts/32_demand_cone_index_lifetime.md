# Demand-cone index lifetime

## Status

The demand-cone dependency and rule indexes are now built once per
`evaluate/4` call and threaded through all evaluator strata. Direct callers
retain the existing `demand_cone_rules/6` wrapper, which builds a private
one-call index. No process-global cache or compiler-kernel behavior changed.

## Signatures and storage

The evaluator path is now:

```prolog
evaluate/4
  -> demand_cone_static_indexes(+Strata, +Rules, +Dependencies,
                                -StaticIndexes)
  -> evaluate_strata(+Level, +MaxStratum, +Strata, +Dependencies,
                     +StaticIndexes, +Rules, +Seeds, +LowerRows,
                     -Closure, -Diagnostics)
  -> demand_cone_rules_indexed(+Level, +StaticIndexes, +CurrentRules,
                               -PlainRules)
```

The compatibility entry point remains:

```prolog
demand_cone_rules(+Strata, +Level, +Rules, +Dependencies,
                  +CurrentRules, -PlainRules)
```

`StaticIndexes` contains:

```prolog
static_indexes(DependencyIndex, RuleIndex)
```

`DependencyIndex` is unchanged and contains only exact
`dependency(Head, Body, positive, 0, positive)` edges. `RuleIndex` now stores
`rule_definition(RuleLevel, Rule)` entries for every nonaggregate rule. The
`RuleLevel =< Level` filter runs at body-relation discovery, allowing the same
index to serve every stratum while retaining all duplicate terms and same-head
definitions.

The static term is created inside the dynamic extent of one `evaluate/4` call,
passed through recursive strata, and becomes unreachable after the call. Nested
evaluations construct independent terms.

## Pre-edit index-build measurement

On nearest-shadow, the prior implementation rebuilt both indexes for each
stratum call. Direct per-level measurements of those two builds were:

| level | current rules | index-build inferences |
| ---: | ---: | ---: |
| 0 | 29 | 2,261 |
| 1 | 13 | 2,398 |
| 2 | 40 | 2,726 |
| 3 | 5 | 2,771 |
| 4 | 3 | 2,810 |
| 5 | 27 | 2,996 |
| 6 | 3 | 3,014 |
| total | 120 | 18,976 |

The nearest-shadow cold compile immediately before this change measured
2,040,156 inferences. The repeated index construction accounted for 18,976
direct helper inferences per single pass over the seven levels; the full
compiler invokes evaluator strata across compile rounds as well.

## Exact-output parity

Added `demand_cone_static_indexes_match_wrapper_at_every_fixture_level`, which
compares the reusable indexed path with the compatibility wrapper at every
stratum level for nearest-shadow and `2_partial`.

Canonical SHA-256 values remained unchanged:

| fixture | rows | hash |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

## Performance measurements

Nearest-shadow used three fresh `compiler-perf-gate` processes. The previous
indexed-demand-cone path measured 2,040,156 cold inferences and a 444 ms
median wall. This lifetime path measured 2,010,858 cold inferences on all
three runs. Wall times were 432, 493, and 488 ms, median 488 ms. The wall
spread is process-load variation; the inference delta is -29,298 (-1.44%).
The gate passed with 810 rows and empty diagnostics. Warm output remained
2,236 inferences and 4 ms.

The traced `2_partial` profile reached 910 rows, eight closure rounds, empty
diagnostics, 4,493,644 cold inferences, and 1,262 ms. Its existing
`compiler_row_checkpoint(910,15562)` condition still exits 1; comparator tests
pass independently.

## Validation

| command | result |
| --- | --- |
| static/wrapper parity at all fixture levels | passed |
| focused demand-cone and evaluator tests | passed |
| `v7/test/18_binding_symmetry.test.pl` | 16/16 passed |
| `v7/test/19_lexical_binding.test.pl` | 10/10 passed |
| `v7/test/20_compiler_performance.test.pl` | 17/17 passed |
| `v7/test/21_compiler_profile.test.pl` | 25/25 passed |
| nearest-shadow live compiler gate | passed; 810 rows, empty diagnostics |
| `2_partial` live compiler profile | 910 rows, 8 rounds; existing checkpoint failure |

No CI files or CI coverage were changed.
