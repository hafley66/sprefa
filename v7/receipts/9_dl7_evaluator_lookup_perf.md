# DL7 evaluator lookup performance: validation grouping and ground-argument index

Date: 2026-09-10
Lane: `feature-dl7-evaluator-perf-f41-20260910`
Base: `b900b80f2dc0c22f893bfab5963bca29c79b5d41`
Parent: `codex-2344`

## TOC

- [Scope](#scope)
- [Inherited baseline vs owned change](#inherited-baseline-vs-owned-change)
- [Gate 1: validation grouping](#gate-1-validation-grouping)
- [Gate 2: ground-argument index](#gate-2-ground-argument-index)
- [Commands and actual output](#commands-and-actual-output)
- [Exact-output evidence](#exact-output-evidence)
- [Inference and wall evidence](#inference-and-wall-evidence)
- [Live JITI evidence](#live-jiti-evidence)
- [Tests](#tests)
- [Status](#status)

## Scope

Two accepted gates in one file:

- Gate 1: relation-row grouping plus grouped functional-key validation in
  `validate_functional_rows/3`.
- Gate 2: a ground-argument index over the lower-row store, behind the existing
  `evaluation_lower/3` lookup modes.

Argument hashing and indexing change no evaluation, stratification, or phase
semantics. No second cache was added. Trace changes remain a later gate.

Files edited by this lane:

| File | Owned change |
| --- | --- |
| `v7/src/1_libtime/0_evaluator.pl` | grouping + index |
| `v7/test/1_entrypoints.test.pl` | 16 focused tests + 2 helpers |

## Inherited baseline vs owned change

Inherited WIP copied verbatim from the read-only parent checkout
`.../feature/dl7-stratum-memo-f41-20260910` before any implementation. These
are never attributed to this lane and are untouched since the copy (re-hashed
equal after all edits):

| Inherited file | sha256 (copy == current) |
| --- | --- |
| `v7/bench/0_compiler_performance.pl` | `3ce87a3654ef972a0d6d6b6c6586f38024142bcc26333732f703fdbf74dbe77a` |
| `v7/src/0_reader/1_expander.pl` | `866e7a4b21b3630a22a49e1e7ff12731a4afb519604dc681221385d2d82eff7d` |
| `v7/src/0_reader/1a_syntax_grapher.pl` | `13f0b685e2334e9f9fb91fee4ac7798aa78053632fc2d290bcc65bb4c3414ba1` |
| `v7/src/2_comptime/0_lowerer.pl` | `08dd639b771e3a41770cef37a23a318587615d6a45bddfd4a2fa8b5462be6aed` |
| `v7/test/19_lexical_binding.test.pl` | `4eb8f5a7763deffea390a8e881ad22ebc9a361bfc9d2b873f5e63fdcf5c90571` |
| `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7` | `d7b421c9bdbcc3f7bab3aa01a9affe9447dcbe4bebe32c5ae51002191468f100` |
| `v7/AGENTS.md` | `1158b0e8d56213439a3a4d803add13ca9b34131274034dfd804094b18c354fc0` |

Owned hashes:

| File | base sha256 | owned sha256 |
| --- | --- | --- |
| `v7/src/1_libtime/0_evaluator.pl` | `c93ff6da7fa6222e8616f150e7a8f3488f0899c3382a3827c5e42551719ef7a9` | `2dda9131c6b13db4dd0fffa1f0c7c5dc496b6df746c427130a7970988a092253` |
| `v7/test/1_entrypoints.test.pl` | `366a506cf26417529d74efacb29899f6261a0f4213f9242e5ab54a5ed6f41578` | `ad5897710cff7ee0a5c6dbeb84e31040e77df1192a6184d3e0c9d6d070e8500e` |

## Gate 1: validation grouping

Two scans in `validate_functional_rows/3` were restructured. Both preserved
input row order for orientation; the final `sort/2` on diagnostics is unchanged.

1. Relation-row grouping. Original `relation_key_diagnostics/3` called
   `relation_rows/3` once per declared relation, rescanning the whole sorted
   closure N times (O(relations x rows)). Now `rows_by_relation/2` builds one
   `keysort/2` + `group_pairs_by_key/2` map and `list_to_assoc/2` index once;
   each relation reads only its rows via `relation_index_rows/3`.
2. Grouped functional-key validation. Original `functional_key_conflict/4`
   called `ordered_row_pair/3` over all rows, generating every ordered pair
   before comparing key values. Now `key_groups/3` keys each row once and
   `keysort/2` + `group_pairs_by_key/2` groups by key; `ordered_row_pair/3`
   compares pairs only inside one equal-key group. `Left`/`Right` orientation
   is preserved because grouping keeps original relative order (stable
   `keysort/2`).

`ordered_row_pair/3`, `key_values/3`, and `argument_at/3` are unchanged.
`relation_rows/3` was removed (only local caller). No new dependency: only SWI
`library(assoc)` and `library(pairs)`.

An initial version named the indexed worker `relation_key_diagnostics/3` too,
colliding with the wrapper and recursing until the 1 Gb stack limit. The worker
is now `relation_key_diagnostics_indexed/3`.

## Gate 2: ground-argument index

One dynamic store, not two. `:- dynamic evaluation_lower/3.` is gone;
`:- dynamic evaluation_lower_index/7.` replaces it. `evaluation_lower/3` is now
a normal nondeterministic lookup wrapper:

```prolog
evaluation_lower(EvaluationId, Relation, Row) :-
    (   Row = call(_, Arguments)
    ->  index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4)
    ;   Hash1 = Hash2, Hash2 = Hash3, Hash3 = Hash4
    ),
    evaluation_lower_index(EvaluationId, Relation,
                           Hash1, Hash2, Hash3, Hash4, Stored),
    Stored = Row.

index_argument_hash(Position, Arguments, Hash) :-
    (   nonvar(Arguments),
        nth0(Position, Arguments, Argument),
        ground(Argument)
    ->  term_hash(Argument, Hash)
    ;   true
    ).
```

Properties, matching the parent corrections:

- Exactly one index clause per inserted row. `install_lower_rows/3` asserts
  `evaluation_lower_index(EvaluationId, Relation, H1, H2, H3, H4, Row)` and
  records exactly one `ClauseReference`. Corrected from the earlier proposal,
  which would have asserted both a row store and an index row and leaked one
  clause per row.
- Cleanup ownership unchanged: references flow through `install_evaluation/5`
  and are erased by `clear_evaluation/2` alongside the table subgoals.
- Hash positions 1-4 only, a private physical layout. All arities, including 0
  and greater than 4, are retained in the full `Row`.
- Hashes computed only for ground arguments; unknown or absent positions stay
  unbound. Ground lookup narrows candidates, then `Stored = Row` is mandatory
  exact unification, so a hash collision cannot change an answer.
- Per-`EvaluationId` isolation and wrapper nondeterminism preserved: free `Row`
  or `Relation` enumerates the same rows the old store did.

No `2_partial.dl7` rerun this gate; bounded checks only, per the hard cap.

## Commands and actual output

Run from the repo root. Every compiler/test command is under `timeout 20`;
profile probes under `timeout 15`.

Scratch capture `/private/tmp/dl7_f41_capture.pl` clears caches, compiles
`7_nearest_shadow.dl7`, writes `output(Rows,Runtime,Diagnostics)` canonically to
argv[1], and prints `ms`/`inferences`/`rows`.

```bash
timeout 20 swipl -q -s /private/tmp/dl7_f41_capture.pl -- /private/tmp/dl7_f41_baseline.term
timeout 20 swipl -q -s /private/tmp/dl7_f41_capture.pl -- /private/tmp/dl7_f41_after.term
timeout 20 swipl -q -s /private/tmp/dl7_f41_capture.pl -- /private/tmp/dl7_f41_after2.term
timeout 20 swipl -q -s /private/tmp/dl7_f41_capture.pl -- /private/tmp/dl7_f41_index.term
timeout 20 swipl -q -s /private/tmp/dl7_f41_capture.pl -- /private/tmp/dl7_f41_index2.term
```

Gate 1 grouped evaluator (before gate 2):

```text
baseline    capture ms=7050 inferences=34671190 rows=14586 diagnostics=[]
after       capture ms=6144 inferences=11099311 rows=14586 diagnostics=[]
after2      capture ms=6204 inferences=11099311 rows=14586 diagnostics=[]
```

Gate 2 indexed evaluator:

```text
index       capture ms=2890 inferences=15661262 rows=14586 diagnostics=[]
index2      capture ms=3144 inferences=15661262 rows=14586 diagnostics=[]
```

Artifacts retained: `/private/tmp/dl7_f41_baseline.term`,
`/private/tmp/dl7_f41_after.term`, `/private/tmp/dl7_f41_after2.term`,
`/private/tmp/dl7_f41_index.term`, `/private/tmp/dl7_f41_index2.term`.

## Exact-output evidence

```text
cmp baseline after    -> IDENTICAL_BYTES
cmp baseline after2   -> IDENTICAL_BYTES
cmp baseline index    -> IDENTICAL_BYTES
cmp baseline index2   -> IDENTICAL_BYTES
```

The complete `output(Rows,Runtime,Diagnostics)` term is byte-identical cold
across the base, the grouped evaluator, and the indexed evaluator. Rows
`14586`, diagnostics `[]`.

## Inference and wall evidence

`nearest-shadow` (`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`),
cold cache cleared each run:

| Metric | Base | Gate 1 grouped | Gate 2 indexed |
| --- | --- | --- | --- |
| wall ms | 7050 | 6144 / 6204 | 2890 / 3144 |
| inferences | 34,671,190 | 11,099,311 | 15,661,262 |
| rows | 14586 | 14586 | 14586 |
| diagnostics | `[]` | `[]` | `[]` |

Gate 1 removed ~23.6 M inferences with a small wall gain; gate 2 removes a
further ~3 s of wall at the cost of ~4.6 M index-maintenance inferences. Both
runs land near the three-second per-case goal, which is not yet met with
margin. One fixture; no corpus-wide claim. The gate-1 `2_partial.dl7` table
(cold 59,593,729 -> 33,720,845 inferences, 36,785 -> 34,295 ms, warm 2,339
both, rows 15,542, rounds 8) is retained from gate 1 and was not rerun under
the gate-2 cap.

## Live JITI evidence

`/private/tmp/dl7_f41_jiti_probe.pl` installs 200 single-argument rows, runs
ground, partial, and free lookups, then calls `library(prolog_jiti):jiti_list/1`
before cleanup. `timeout 15`:

```text
JITI evaluation_lower_index/7:
Predicate                                 #Clauses  Index  Buckets  Speedup  Coll Flags
dl7_evaluator:evaluation_lower_index/7    200       3      256      200.0    47
                                                    2      2        1.0      -   L
                                                    2:1    2        1.0      -   L
```

JITI builds a 256-bucket index with a measured `Speedup 200.0` on the probe and
records `47` collisions. The forced collision test below proves those
collisions are harmless because exact full-row unification is the last filter.

## Tests

Actual regression command from the `v7/justfile` convention, run from the repo
root under `timeout 20`:

```bash
timeout 20 swipl -q -s v7/test/1_entrypoints.test.pl \
  -g "run_tests([<names>])" -t halt
```

Gate-1 validation seam (edit target reported before editing:
`v7/test/1_entrypoints.test.pl`, the evaluator oracle). All passed:

```text
final_closure_rejects_declared_functional_key_conflicts
functional_validation_empty_rows_has_no_diagnostics
functional_validation_distinct_keys_has_no_diagnostics
functional_validation_equal_key_conflict_keeps_row_orientation
functional_validation_checks_every_declared_key
functional_validation_reports_every_ordered_equal_key_pair
functional_validation_scopes_conflicts_per_relation
functional_validation_missing_relation_has_no_rows
functional_validation_relation_without_keys_is_skipped
functional_validation_orientation_follows_sorted_rows
functional_validation_deduplicates_repeated_rows
functional_validation_repeated_rows_do_not_hide_conflicts
```

Gate-2 index seam. All passed:

```text
evaluation_index_enumerates_free_row_and_relation      free Row + free Relation
evaluation_index_matches_partial_and_ground_arguments  partial arg, ground membership
evaluation_index_retains_arity_zero_and_extra_arguments arity 0 and arity 5
evaluation_index_exact_unification_rejects_forced_collision forced duplicate hash
evaluation_index_is_cleared_across_lifecycle_paths      success + strict-cycle failure
```

Evaluator lifecycle cases rerun green from the repo root:
`stratification_is_pure_deterministic_and_strict_cycle_checked`,
`cons_constructs_deconstructs_and_stops_at_the_empty_tail`,
`prefix_negation_is_safe_stratified_and_cleanup_scoped`.

`evaluator_snapshot/1` now counts `evaluation_lower_index/7` directly, so the
existing success, strict-cycle failure, and exception lifecycle assertions
(`evaluator_exception_receipt/1`) all verify the index store is empty after
`evaluate/4` and after a thrown exception. The forced-collision case asserts a
ground lookup returns only the matching row while enumeration returns both
stored rows.

CI coverage change: 16 new test cases added (11 gate 1, 5 gate 2); no test
removed or changed except the snapshot counter switching to the new store.

Full-file `run_tests` of `1_entrypoints.test.pl` is not reported: the earlier
280 s attempt violated the cap and reached 24/45 before it. Pre-existing cases
run 7-29 s each. Not rerun this gate.

## Status

Gate 1 accepted; gate 2 implemented and stopped for parent review. No commit,
push, merge, tracer change, or evaluator-semantics change. Tracer instrumentation
remains the subsequent gate.
