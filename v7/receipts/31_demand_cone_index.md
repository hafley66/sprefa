# Demand-cone relation indexes

## Status

`demand_cone_rules/6` now constructs two immutable assoc indexes per stratum
call and visits each discovered relation head once. The selected rule terms are
still finalized with the existing `sort/2` boundary. No compiler kernel,
binding, type, evaluation, graph, or output contract changed. No commit or
push was made.

## Exact predicates and modes

The public evaluator path remains:

```prolog
demand_cone_rules(+Strata, +Level, +Rules, +Dependencies,
                  +CurrentRules, -PlainRules)
```

The changed internal path is:

```prolog
demand_cone_dependency_index(+Dependencies, -DependencyIndex)
demand_cone_rule_index(+Strata, +Level, +Rules, -RuleIndex)
demand_cone_worklist(+Queue, +DependencyIndex, +RuleIndex,
                     +SeenRelations, +IncludedRelations, +Selected0,
                     -Selected)
demand_cone_discover(+BodyRelations, +RuleIndex,
                     +Seen0, -Seen, +Included0, -Included,
                     -NewRelations, -NewRules)
```

`DependencyIndex` stores only exact
`dependency(Head, Body, positive, 0, positive)` edges, grouped by `Head` and
sorted per group. `RuleIndex` stores each nonaggregate rule whose exact head
relation has a stratum level less than or equal to the current level. A body
relation can already be a current root, so queue membership and first rule
definition inclusion use separate assoc sets. Same-head definitions therefore
remain available, while exact duplicate terms are removed only by the existing
final `sort/2`.

## Duplicate-work categories

The previous path repeated these operations at every selected-rule fixpoint
pass:

| category | previous operation | indexed path |
| --- | --- | --- |
| selected rules × dependencies | rescan all selected rules and all dependency terms | one grouped dependency index, one relation lookup |
| discovered relations × rules | rescan all rules with `include/3` | one grouped eligible-rule index, one inclusion per body relation |
| cycles and diamonds | rediscover already-seen relation heads | assoc seen set and one queue entry per relation |
| output ordering | sort each fixpoint pass | one final `sort/2` |

The prior nearest-shadow instrumentation measured `demand_cone_rules/6` at
431,151 inclusive inferences across 15 calls. The synthetic chain below uses
the test-local pre-change fixpoint as the old path and the indexed evaluator
path as the new path.

| chain relations | old inferences | old wall (s) | new inferences | new wall (s) |
| ---: | ---: | ---: | ---: | ---: |
| 100 | 602,592 | 0.042774 | 11,541 | 0.001128 |
| 200 | 4,404,542 | 0.323596 | 24,089 | 0.002418 |
| 400 | 33,608,442 | 2.466108 | 50,801 | 0.006033 |

## Canonical output parity

The canonical term was
`write_canonical(output(Rows, Runtime, Diagnostics))`, hashed with SHA-256 in
separate SWI processes.

| fixture | rows | indexed hash |
| --- | ---: | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` |

The focused synthetic parity test compares the indexed result against the
pre-change fixpoint for same-head definitions, duplicate rule terms, positive
cycles, diamonds, lower-level dependencies, current rules without
dependencies, future-level rules, and positive/negative/aggregate/strict edge
filters. Existing evaluator tests cover generated `Option(text)`, recursion,
lower snapshots, negation, aggregate reads, cleanup, and nested evaluation.

## Whole-compile measurements

Nearest-shadow measurements used three fresh `just compiler-perf-gate` runs.
The immediately preceding checkout measurement was 2,403,099 cold inferences
and 443 ms from receipt 29. The indexed path was 2,040,156 cold inferences on
each run; wall times were 447, 434, and 444 ms, median 444 ms. The gate passed
with 810 rows and empty diagnostics. Warm output was 2,236 inferences and 4
ms.

The traced `2_partial` live profile reached 910 rows, eight closure rounds,
empty diagnostics, 4,602,412 cold inferences, and 1,268 ms. It retains the
existing `compiler_row_checkpoint(910,15562)` exit-1 condition. The comparator
test suite passes independently.

## Validation

| command | result |
| --- | --- |
| focused demand-cone and evaluator tests | passed |
| `v7/test/18_binding_symmetry.test.pl` | 16/16 passed |
| `v7/test/19_lexical_binding.test.pl` | 10/10 passed |
| `v7/test/20_compiler_performance.test.pl` | 17/17 passed |
| `v7/test/21_compiler_profile.test.pl` | 25/25 passed |
| nearest-shadow live compiler gate | passed; 810 rows, empty diagnostics |
| `2_partial` live compiler profile | 910 rows, 8 rounds; existing checkpoint failure |

No CI files or CI coverage were changed.
