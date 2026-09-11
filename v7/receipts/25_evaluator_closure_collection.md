# Evaluator closure collection: bound current-stratum roots

## Status

Implemented and validated in the shared checkout. `collect_closure/4` now
enumerates only current-stratum rule-head and seed relations with the relation
argument bound, then ordered-unions those new rows and generated requests with
the completed lower snapshot. Installed rules, demand-cone selection, and
kernel semantics are unchanged. No commit or push.

## Exact predicates changed

| File | Predicates |
| --- | --- |
| `v7/src/1_libtime/0_evaluator.pl` | `evaluate_strata/9`, `evaluate_stratum_after_aggregates/14`, new `current_result_relations/3`, `collect_closure/4`, `profile_closure_rows/1` |
| `v7/test/1_entrypoints.test.pl` | new `evaluator_collection_binds_result_relations_and_unions_lower_rows` test |

`current_result_relations/3` always includes `ref(kernel(nil))`, current rule
heads, and current seed relations. `collect_closure/4` sorts current proofs and
requests, applies `ord_union/3`, and merges them with `LowerRows`. Profiling now
records only rows collected for the current stratum under the existing
`evaluator_collected_closure_rows` category.

## Measured nearest-shadow gate

Five cold runs after the change produced wall values `504, 499, 499, 514, 533`
ms, median `504 ms`. The prior shared-lower-store baseline was `2,978,066`
inferences and `553 ms` median over nine runs.

| | inferences | cold wall median | rows | diagnostics |
| --- | ---: | ---: | ---: | --- |
| before | 2,978,066 | 553 ms (n=9) | 810 | `[]` |
| after | 2,927,019 | 504 ms (n=5) | 810 | `[]` |
| delta | -51,047 (-1.71%) | -49 ms (-8.86%) | 0 | unchanged |

The performance gate reports cold `2,927,019` inferences, `504 ms`, warm
`2,240` inferences, and `810` rows. The untraced nearest-shadow profile has no
closure-round field. Its configured cold budgets remain 16,000,000 inferences
and 3,000 ms.

The traced `2_partial` profile produced `6,557,302` cold inferences and `1,430
ms` median wall over three runs, with `2,422` warm inferences, `910` rows, and
eight closure rounds. The prior shared-lower-store receipt recorded `6,423,729`
cold inferences and `1,108 ms` median wall over twelve runs. The live row output
and canonical bytes remain unchanged; the existing live CLI checkpoint still
expects `15,562` rows.

## Duplicate occurrence categories

Nearest-shadow profile output after the change:

| category | total | unique | duplicate | percent |
| --- | ---: | ---: | ---: | ---: |
| `evaluator_installed_lower_rows` | 2,679 | 1,341 | 1,338 | 49.94 |
| `evaluator_installed_rules` | 452 | 125 | 327 | 72.35 |
| `evaluator_installed_seeds` | 2,901 | 1,433 | 1,468 | 50.60 |
| `evaluator_collected_closure_rows` | 2,923 | 1,438 | 1,485 | 50.80 |
| overall | 8,976 | 4,346 | 4,630 | 51.58 |

The prior closure category was `11,741` total, `1,438` unique, `10,303`
duplicate (`87.75%`). The collection category therefore removes `8,818`
repeated lower-snapshot occurrences while preserving the same distinct rows.

## Canonical output parity

The canonical term was `write_canonical(output(Rows, Runtime, Diagnostics))`
hashed with SHA-256 in separate SWI processes. The pre-change unbound collector
and the bound collector produced identical hashes in this checkout:

| Fixture | rows | before hash | after hash | result |
| --- | ---: | --- | --- | --- |
| `7_nearest_shadow.dl7` | 810 | `d23315e1c3148b13ff8697f0b0b2a51a94cba7c1762ae4081cc1bdc4bddf5186` | same | identical |
| `2_partial.dl7` | 910 | `8dd2d7dd2571fc18a571e58898cc6e48bbb7c1de2badfb59449dbf8457999748` | same | identical |

## Validation

| Command | Result |
| --- | --- |
| focused evaluator subset in `v7/test/1_entrypoints.test.pl` | 16/16 passed; slowest 0.012 s |
| `v7/test/20_compiler_performance.test.pl` | 17/17 passed |
| `v7/test/18_binding_symmetry.test.pl` | 16/16 passed; slowest 0.960 s |
| `v7/test/19_lexical_binding.test.pl` | 10/10 passed |
| `v7/test/21_compiler_profile.test.pl` | 25/25 passed; slowest 1.551 s |
| nearest-shadow compiler gate | pass; 810 rows, empty diagnostics |
| `2_partial` compiler gate | live CLI reaches 910 rows and exits 1 on the existing `compiler_row_checkpoint(910,15562)`; the 17 comparator tests pass, including the injected 15,562-row checkpoint |

All commands were run one SWI process at a time under `timeout 20`.
