# Evaluator duplicate work: shared lower-row store per evaluate/4 call

## Status

Implemented and validated. One repeated evaluator operation removed: the same
immutable lower row was re-asserted into `evaluation_lower_index/7` once per
stratum `EvaluationId` within a single `evaluate/4` call. The rows now live in
one evaluate-scope store, installed by `ord_subtract/3` delta. Byte-exact
compiler output is preserved; cold inferences drop. No commit, push, or merge.

- Base SHA: `312563dbd1671794e4ddabbece54cadfd5b38f81`
- Next: parent

## Files

| File | Change |
| --- | --- |
| `v7/src/1_libtime/0_evaluator.pl` | evaluate-scope shared lower-row store; delta install; `evaluation_lower/3` store resolution; 3 focused test companions |
| `v7/test/1_entrypoints.test.pl` | 3 new tests (dedup, scope cleanup, shared collision) |

No unowned file touched.

## Operation removed

Path: `evaluate_strata/…` -> `evaluate_stratum_after_aggregates/…` ->
`install_evaluation/5` -> `install_lower_rows/3` -> `install_lower_rows_plain/3`
-> `assertz(evaluation_lower_index(EvaluationId, Relation, H1..H4, Row))`.

`LowerRows` for stratum N is the completed closure of strata `0..N-1`, so each
stratum re-asserted every earlier stratum's rows under its own `EvaluationId`,
plus 4 `term_hash/2` calls per row. Stratum snapshots are nested and monotone
within one `evaluate/4` call, and only one `EvaluationId` is live at a time.

Change: `evaluate/4` opens one lower store (`setup_call_cleanup/3`), and
`install_shared_lower_rows/3` asserts only `Rows \ Installed` via
`ord_subtract/3`. Each stratum keeps its own rules, seeds, SLG table, request
table, and proof identity under its own `EvaluationId`; `evaluation_lower/3`
resolves the shared store through `lower_store_id/2`. Outside a scope the
`EvaluationId` is its own store, so direct `install_lower_rows` callers and
simultaneous-`EvaluationId` isolation are unchanged. `clear_evaluation/2` does
not erase shared rows; `close_lower_store/0` retracts them on success, failure,
and exception. Nested scopes unwind LIFO (probe verified).

## Inference and wall

Unprofiled cold, same worktree path, `capture.pl` writes canonical
`output(Rows, Runtime, Diagnostics)`.

| Fixture | | inferences | wall ms (median) | rows |
| --- | --- | --- | --- | --- |
| 7_nearest_shadow | before | 3,136,313 | 555 (n=9) | 810 |
| 7_nearest_shadow | after | 2,978,072 | 553 (n=9) | 810 |
| 2_partial | before | 7,035,916 | 1,099 (n=5) | 910 |
| 2_partial | after | 6,423,729 | 1,108 (n=12) | 910 |

Inference delta: nearest-shadow `-158,241` (-5.05%); 2_partial `-612,187`
(-8.70%). Median wall is flat within run noise. `compiler-perf-gate`: cold
`inferences=2,978,066 wall_ms=560` (budget 16,000,000 / 3,000), warm
`inferences=2,236`, rows 810, empty diagnostics.

## Duplicate occurrence counts

Existing profiler, nearest-shadow. `2_duplicates.tsv` before -> after:

| category | total | unique | duplicate | percent |
| --- | --- | --- | --- | --- |
| evaluator_installed_lower_rows | 8830 -> 2679 | 1341 -> 1341 | 7489 -> 1338 | 84.81 -> 49.94 |
| evaluator_installed_rules | 452 | 125 | 327 | 72.35 |
| evaluator_installed_seeds | 2901 | 1433 | 1468 | 50.60 |
| evaluator_collected_closure_rows | 11741 | 1438 | 10303 | 87.75 |
| overall | 23945 -> 17794 | 4346 | 19599 -> 13448 | 81.85 -> 75.58 |

6151 repeated lower-row assertions removed across the compile's three
`evaluate/4` calls (macrotime, round 1, round 2). Remaining 1338 lower-row
duplicates are rows repeated across separate evaluate calls, which are not
shared. Profiled total inferences `3,753,882 -> 3,534,140`.

## Canonical hashes

`write_canonical(output(Rows, Runtime, Diagnostics))`, nearest-shadow and
2_partial:

| Fixture | before | after | compare |
| --- | --- | --- | --- |
| 7_nearest_shadow | `57536fd49414e11dbd4fb99b0431a48f44e121e5d34d8a1d3edf1e6702bc628c` | same | IDENTICAL_BYTES |
| 2_partial | `c9f5e83a24029fc236b7222f18ba0263ba0a729123fbbdd8fbf2af600fb90d1e` | same | IDENTICAL_BYTES |

Owned file hashes: `0_evaluator.pl` `0a7b01ab9462dfd9…`, `1_entrypoints.test.pl`
`755efd8bdb1b6d6d…`.

## Tests

Run from the repo root under `timeout 20`.

| Command | Result | Slowest |
| --- | --- | --- |
| 19 evaluator index/lifecycle/trace/negation/aggregate tests | 19/19 passed | 0.011 s |
| 3 new shared-store tests + 3 index lifecycle/collision/isolation | 6/6 passed | 0.012 s |
| `just -f v7/justfile compiler-perf-test` | 17/17 passed | 0.118 s |
| `timeout 20 swipl -q -s v7/test/3_compiler_trace.test.pl -g run_tests -t halt` | 15/15 passed | 0.240 s |
| `just -f v7/justfile compiler-perf-gate` | passed | 0.560 s |

New: `evaluation_shared_lower_store_deduplicates_growing_snapshots` (stored
clause count 2 then 3 across a growing snapshot, empty reference list, exact
enumeration), `evaluation_shared_lower_store_clears_on_scope_exit` (no rows or
scopes left), `evaluation_shared_lower_store_rejects_forced_collision` (a forced
duplicate hash still filters by `Stored = Row`). Every case is well under 3 s.

## Semantic surface

Unchanged: relation closure, diagnostics, row ordering, negation over completed
lower rows, count aggregates, lookup modes (free/partial/ground), per-stratum
SLG table and proof identity, `EvaluationId` isolation outside a scope, cleanup
on success/failure/exception, and nested-scope unwinding. The demand-cone rule
selection and the prior `Option(text)` mode-sensitive path are untouched; both
canonical fixtures are byte-identical, and the five
`evaluator_current_stratum_*` mode tests pass. This is an internal
representation substitution only.

## Next

parent review.
