# DL7 evaluator trace: wrapper simplification and opt-in stratum tracing

Date: 2026-09-10
Lane: `feature-dl7-evaluator-trace-f41-20260910`
Base: `b900b80f2dc0c22f893bfab5963bca29c79b5d41`
Parent: `codex-2344`

## TOC

- [Scope](#scope)
- [Review checks adopted](#review-checks-adopted)
- [Inherited baseline vs owned change](#inherited-baseline-vs-owned-change)
- [Task 1: lookup wrapper simplification](#task-1-lookup-wrapper-simplification)
- [Task 2: pre-edit profile and tracing signature](#task-2-pre-edit-profile-and-tracing-signature)
- [Task 3: opt-in stratum tracing](#task-3-opt-in-stratum-tracing)
- [Task 4: trace on/off measurements](#task-4-trace-onoff-measurements)
- [Exact-output evidence](#exact-output-evidence)
- [Tests](#tests)
- [Remaining bottleneck](#remaining-bottleneck)
- [Scoped diff](#scoped-diff)
- [Status](#status)

## Scope

Gate 3 of the F41 evaluator arc, on top of the gate-2 ground-argument index
(receipt 9):

- Simplify the `evaluation_lower/3` lookup wrapper: every stored row is a
  `call/2`, so the fallback that equated all four hash positions is dead.
- Add one isolation test proving two simultaneous `EvaluationId`s do not mix
  and that cleaning one leaves the other intact.
- Add opt-in tracing through the existing
  `dl7_compiler_tracer:run_compile_step/4` for install, collect, and cleanup per
  stratum plus functional validation.

No evaluation, stratification, or phase semantics changed. Installed extract
static queries remain a separate parent task. No commit, push, merge, or
delegation.

## Review checks adopted

1. **Generic trace phase, no orphan ledger.** Evaluator steps use phase
   `evaluator`; the compiler's outer `evaluate_round` step keeps phase
   `comptime`. `compile_step_trace_on/0` in the tracer now requires
   `active_compile_trace(_)` as well as `DL7_TRACE` in
   `[steps,json,collect]`, so a standalone `evaluate/4` call (runtime or test,
   no `with_compile_trace/2`) never measures or asserts step rows. Focused test
   `evaluator_trace_is_gated_on_an_active_compile_trace` covers it.
2. **Real `table_statistics/2` keys, accurate names.** The collect metrics read
   only documented keys: `answers`, `complete_call`, and `space`.
   `complete_call` is documented as "number of times answers are generated from
   a completed table, i.e., times answers are reused"; it is a process-global
   counter, not a per-stratum cache-hit count. The metrics are named
   `global_table_answers`, `global_complete_calls`,
   `global_table_space_bytes`. No invented key; a missing/unsupported stat is
   not reported as zero reuse because the whole metrics callback fails and is
   dropped to `[]`. The `table_statistics/2` calls happen on the collect step,
   before cleanup abolishes the tables.
3. **No repeated whole-store enumeration.** Install metrics use the known input
   list lengths (`stratum_rules`, `stratum_seeds`, `stratum_lower_rows`) and do
   not touch the store; there is no `StoreRows` findall. Cleanup reports the
   known clause-reference count and a per-`EvaluationId` leak check via
   `aggregate_all(count, evaluation_lower_index(EvaluationId, ...), Count)`,
   first argument bound, without materializing row bags. Metrics run only when
   an active trace has step collection enabled; trace OFF does no metric work.
   `finish_compile_step/4` was hardened so a throwing or failing metrics
   callback falls back to `[]` without a partial-binding hazard, keeping the
   step row and the surrounding cleanup intact.

## Inherited baseline vs owned change

Ten paths were copied verbatim from the read-only perf checkout
`.../feature/dl7-evaluator-perf-f41-20260910` (`b900b80f2` plus WIP) before any
edit. These are never attributed to this lane and are byte-identical to the
source after all edits (`diff -q` clean):

| Inherited file | sha256 (copy == current) |
| --- | --- |
| `v7/bench/0_compiler_performance.pl` | `3ce87a3654ef972a0d6d6b6c6586f38024142bcc26333732f703fdbf74dbe77a` |
| `v7/src/0_reader/1_expander.pl` | `866e7a4b21b3630a22a49e1e7ff12731a4afb519604dc681221385d2d82eff7d` |
| `v7/src/0_reader/1a_syntax_grapher.pl` | `13f0b685e2334e9f9fb91fee4ac7798aa78053632fc2d290bcc65bb4c3414ba1` |
| `v7/src/2_comptime/0_lowerer.pl` | `08dd639b771e3a41770cef37a23a318587615d6a45bddfd4a2fa8b5462be6aed` |
| `v7/test/19_lexical_binding.test.pl` | `4eb8f5a7763deffea390a8e881ad22ebc9a361bfc9d2b873f5e63fdcf5c90571` |
| `v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7` | `d7b421c9bdbcc3f7bab3aa01a9affe9447dcbe4bebe32c5ae51002191468f100` |
| `v7/AGENTS.md` | `1158b0e8d56213439a3a4d803add13ca9b34131274034dfd804094b18c354fc0` |
| `v7/receipts/9_dl7_evaluator_lookup_perf.md` | `f5656503d10ac24dd5c29179b6470ce7809e600115cddb3d821fb28f6a25bf7a` |

Four owned files. `0_evaluator.pl` and `1_entrypoints.test.pl` baseline is the
gate-2 checkout; the tracer and its test baseline is repo HEAD `b900b80f2`.

| Owned file | baseline sha256 | owned sha256 |
| --- | --- | --- |
| `v7/src/1_libtime/0_evaluator.pl` | `2dda9131c6b13db4dd0fffa1f0c7c5dc496b6df746c427130a7970988a092253` | `5c02bc0b3040279b2ed70a52b126702e6100dee9ebc74a94714785b6459f4647` |
| `v7/test/1_entrypoints.test.pl` | `ad5897710cff7ee0a5c6dbeb84e31040e77df1192a6184d3e0c9d6d070e8500e` | `cbbb89d543ead59b7ef128f8caf08f3d14f9af39df9ca0986d2d5be4f5223484` |
| `v7/src/2_comptime/1b_compiler_tracer.pl` | `444b1f936b6ac65d065e7bba7dcb2ff9df64f69ef4d555541e1179c63144ff7d` | `c39e79b94128bbc8ba80e892358cc32ba6a238e5f419850a77e8dac38e66704d` |
| `v7/test/3_compiler_trace.test.pl` | `be803d40d4ca4ed4f159cd06eb6bf24f6e58a6cee4d27c69317892d588a30488` | `31131bc270f1f1e5ef49bbada2e97b5c9bbb810666e1dde477b07de7d651fd29` |

## Task 1: lookup wrapper simplification

`evaluation_lower/3` previously guarded `Row = call(_, Arguments)` and, in the
else branch, equated all four hash positions before querying the index. Since
`install_lower_rows/3` only ever asserts `call/2` rows, the else branch could
never select a stored row (`Stored = Row` cannot unify a call with a non-call),
so it only produced a failure. The wrapper now decomposes `Row` directly.

```prolog
evaluation_lower(EvaluationId, Relation, Row) :-
    Row = call(_, Arguments),
    index_argument_hashes(Arguments, Hash1, Hash2, Hash3, Hash4),
    evaluation_lower_index(EvaluationId, Relation,
                           Hash1, Hash2, Hash3, Hash4, Stored),
    Stored = Row.
```

All lookup modes are preserved: free `Row` binds to `call(_, Arguments)` and
enumerates; free `Relation` enumerates across relations; ground arguments
contribute hashes that only narrow candidates, and `Stored = Row` remains the
mandatory exact-unification filter.

Isolation test `evaluation_index_isolates_simultaneous_evaluation_ids` installs
one row under each of two `EvaluationId`s, confirms each query sees only its own
row, erases the first reference, and confirms the first store is empty while the
second still returns its row.

## Task 2: pre-edit profile and tracing signature

Profiled the current indexed evaluator before any edit. Command:

```bash
timeout 15 swipl -q -s /private/tmp/dl7_f41_trace_profile.pl
```

Result: `Number of nodes: 421741`, total 3.337 s, `rows=14586`,
`diagnostics=[]`. Top 5 by self wall:

```text
1 dl7_evaluator:evaluation_lower_index/7  0.81s (24.2%)  15,081 calls + 342,948 redos
2 $memberchk/3                            0.39s (11.8%)  275,425 calls, 136,419 fails
3 lists:member_/3                         0.27s ( 8.0%)  23,386 calls + 118,855 redos
4 $tbl_wkl_add_answer/4                   0.23s ( 6.8%)
5 assertz/2                               0.19s ( 5.6%)  179,062 calls
```

`index_argument_hash/3` also ran 757,084 calls for 0.05 s self plus 0.17 s
children. The lookup index remains the single largest self cost.

Dependency check before importing: `dl7_compiler_tracer` imports only
`library(http/json)` and `library(tableutil)`; nothing in `1_libtime` is
imported by the tracer, so evaluator -> tracer introduces no load cycle. The
numeric directory order (`1_libtime` below `2_comptime`) is inverted, but the
tracer is a leaf.

Tracing signature adopted (existing seam, no new framework):

```text
run_compile_step(evaluator, evaluate_install(Level),  install_evaluation(...),  evaluate_install_metrics(...))
run_compile_step(evaluator, evaluate_collect(Level),  collect_closure(...),     evaluate_collect_metrics(...))
run_compile_step(evaluator, evaluate_cleanup(Level),  clear_evaluation(...),    evaluate_cleanup_metrics(...))
run_compile_step(evaluator, validate_functional_rows, validate_functional_rows_body(...), validate_functional_rows_metrics(...))
```

`MetricsGoal` is called as `MetricsGoal(-Metrics)` after the goal succeeds,
outside the measured interval. Metrics run only when an active compile trace has
`DL7_TRACE` in `[steps,json,collect]`; the tracer's off branch never calls
`MetricsGoal`.

## Task 3: opt-in stratum tracing

Edits in `v7/src/1_libtime/0_evaluator.pl`:

- Import `library(aggregate):aggregate_all/3`,
  `library(tableutil):table_statistics/2`, and
  `../2_comptime/1b_compiler_tracer:run_compile_step/4`.
- Wrap the per-stratum `setup_call_cleanup/3` install, collect, and cleanup
  goals in `run_compile_step/4` under phase `evaluator`.
- Split `validate_functional_rows/3` into the traced wrapper and
  `validate_functional_rows_body/3`, plus
  `validate_functional_rows_metrics/4`.
- Add three metrics predicates.

Edits in `v7/src/2_comptime/1b_compiler_tracer.pl`:

- `compile_step_trace_on/0` now requires `active_compile_trace(_)`.
- `finish_compile_step/4` binds a fresh `Collected`, checks `is_list/1`, and
  falls back to `Metrics = []` on failure or throw, so a broken metrics
  callback cannot leave a partially bound list or abort cleanup.

Metrics (computed outside the timed interval):

| Step | Metrics | Scope |
| --- | --- | --- |
| install | `stratum_rules`, `stratum_seeds`, `stratum_lower_rows` | known input list lengths |
| collect | `stratum_closure_rows`, `global_table_answers`, `global_complete_calls`, `global_table_space_bytes` | completed list length + process-global live table counters (`answers`, `complete_call`, `space`) |
| cleanup | `erased_clauses`, `leftover_lower_rows` | known reference count + per-`EvaluationId` leak count via `aggregate_all/3` |
| validate | `validated_relations`, `validated_rows`, `validated_diagnostics` | known input lengths |

No hot-loop printing: all emission stays in the tracer's existing end-of-compile
writers.

## Task 4: trace on/off measurements

Cold `7_nearest_shadow.dl7`, caches cleared per run. Command:

```bash
timeout 20 swipl -q -s /private/tmp/dl7_f41_trace_capture.pl -- <out>
```

Fresh runs at the corrected revision:

| Run | wall ms | inferences | rows | diagnostics |
| --- | --- | --- | --- | --- |
| OFF 1 | 2722 | 15,629,373 | 14586 | `[]` |
| OFF 2 | 2730 | 15,629,373 | 14586 | `[]` |
| OFF 3 | 2786 | 15,629,373 | 14586 | `[]` |
| ON (steps) | 3250 | 16,355,651 | 14586 | `[]` |

All three trace-OFF runs are under the 3 s per-case gate. Trace ON is 3250 ms,
so it was profiled fresh (`DL7_TRACE=collect`, `timeout 15`, total 5.669 s this
run; the profiler total varies with sampling overhead). Remaining top self:
`evaluation_lower_index/7` 22.9%, `trie_gen/3` 12.6%, `$memberchk/3` 8.0%,
`$tbl_wkl_add_answer/4` 6.5%, `lists:member_/3` 6.2%. The extra ON work is
instrumentation: statistics snapshots, global table-stat reads, and cleanup
leak counts.

Representative trace-ON step rows (phase `evaluator`; outer `evaluate_round`
steps stay `comptime`):

```text
step=evaluate_install(4) wall_ms=34 inferences=390792 gc_ms=0 tables=0 stratum_rules=87 stratum_seeds=0 stratum_lower_rows=14451
step=evaluate_collect(4) wall_ms=136 inferences=285961 gc_ms=7 tables=1073 stratum_closure_rows=14451 global_table_answers=28674 global_complete_calls=2791 global_table_space_bytes=6102600
step=evaluate_cleanup(4) wall_ms=17 inferences=50218 gc_ms=0 tables=-1073 erased_clauses=14538 leftover_lower_rows=0
step=validate_functional_rows wall_ms=20 inferences=99964 gc_ms=0 tables=0 validated_relations=130 validated_rows=14586 validated_diagnostics=0
COMPILE-TRACE-STEP program=7_nearest_shadow seq=62 phase=comptime step=evaluate_round(2) ...
```

`leftover_lower_rows=0` on every cleanup step shows the per-`EvaluationId`
store is empty after cleanup. The negative `tables` delta on cleanup is the
existing tracer delta counting the tables abolished during cleanup. Cleanup
inferences dropped from the findall version (~55k per stratum) to ~50k with
`aggregate_all/3`.

## Exact-output evidence

All comparisons use the same worktree path, so the embedded `file(...)` term
cannot differ. `output(Rows, Runtime, Diagnostics)` was written canonically and
compared byte-exact against the gate-2 baseline term:

```text
cmp corr_off1 base(gate2) -> IDENTICAL_BYTES
cmp corr_off1 corr_off2   -> IDENTICAL_BYTES
cmp corr_off1 corr_off3   -> IDENTICAL_BYTES
cmp corr_on   base(gate2) -> IDENTICAL_BYTES
```

Rows `14586`, diagnostics `[]` in every run. Trace ON produces the same
compiler output term as trace OFF and as the gate-2 baseline.

## Tests

Run from the repo root under `timeout 20`. The focused evaluator command:

```bash
timeout 20 swipl -q -s v7/test/1_entrypoints.test.pl \
  -g 'run_tests(dl7_entrypoints:[<names>])' -t halt
```

All passed:

```text
evaluation_index_enumerates_free_row_and_relation
evaluation_index_matches_partial_and_ground_arguments
evaluation_index_retains_arity_zero_and_extra_arguments
evaluation_index_exact_unification_rejects_forced_collision
evaluation_index_is_cleared_across_lifecycle_paths
evaluation_index_isolates_simultaneous_evaluation_ids
evaluator_trace_is_gated_on_an_active_compile_trace
evaluator_trace_reports_stratum_metrics
prefix_negation_is_safe_stratified_and_cleanup_scoped
```

- `evaluator_trace_is_gated_on_an_active_compile_trace`: runs `evaluate/4`
  with `DL7_TRACE=steps` and no active compile trace and asserts
  `compile_step_row/5` stays empty; then runs the same `evaluate/4` on a
  one-rule two-seed program inside `with_compile_trace/2` and asserts the exact
  phase/step keys `evaluator-evaluate_install(0)`,
  `evaluator-evaluate_collect(0)`, `evaluator-evaluate_cleanup(0)` plus install
  `stratum_rules=1`, `stratum_seeds=2`, `stratum_lower_rows=0` and cleanup
  `erased_clauses=3`, `leftover_lower_rows=0`. This is the direct regression for
  the standalone orphan case.
- `evaluator_trace_reports_stratum_metrics`: runs the same program under
  `with_compile_trace/2` with `DL7_TRACE=collect` and pins the install and
  cleanup metric lists exactly, `stratum_closure_rows=5`, and the supported
  global field names `global_table_answers`, `global_complete_calls`,
  `global_table_space_bytes` with integer values (totals not pinned, they are
  machine-dependent process counters).
- Both tests use a local `with_dl7_trace/2` helper that captures the prior
  `DL7_TRACE` value and restores it (unsetenv only when it was unset) before
  resetting the trace, so they do not disturb the environment for other tests.

The tracer file also passes, with one fallback case covering all three
`is_list/1` branches plus a valid control, on a compact shared `metrics_collected/2`
setup that runs each `MetricsGoal` under `with_compile_trace/2`:

```text
timeout 20 swipl -q -s v7/test/3_compiler_trace.test.pl -g run_tests -t halt
step_trace_uses_the_shared_compile_envelope                  passed
json_trace_preserves_phase_step_and_metric_fields           passed
step_metrics_fallback_covers_throw_fail_and_malformed       passed
```

`step_metrics_fallback_covers_throw_fail_and_malformed` feeds a throwing
metrics goal, an ordinary failing goal, a malformed non-list return, and a
valid `[metric(kept,1)]`, asserting the first three record the step with `[]`
and the last records the list. It uses the same env-preserving
`with_dl7_trace/2` helper.

The lifecycle and isolation cases were rerun under `DL7_TRACE=collect` and
passed; the per-`EvaluationId` store and request table were empty afterwards
(`lower=0 req=0`), covering exception/cleanup behavior under tracing.

CI coverage change: three evaluator test cases added
(`evaluation_index_isolates_simultaneous_evaluation_ids`,
`evaluator_trace_is_gated_on_an_active_compile_trace`,
`evaluator_trace_reports_stratum_metrics`) and one tracer test case
(`step_metrics_fallback_covers_throw_fail_and_malformed`); no test removed or
changed. No full-suite or `2_partial.dl7` run (hard cap).

## Remaining bottleneck

For the shipping trace-OFF path the dominant self cost is still
`evaluation_lower_index/7` JITI lookup (24.2% pre-edit, 22.9% under tracing),
followed by `trie_gen/3` (12.6%), `$memberchk/3` (8.0%), and
`lists:member_/3` (6.2%). Tracing does not move that core; its overhead is
confined to snapshots and metric reads that run only when an active trace has
step collection enabled. No further performance rewrite is proposed.

## Scoped diff

`v7/src/1_libtime/0_evaluator.pl`: imports `aggregate_all/3`,
`table_statistics/2`, and `run_compile_step/4`; wraps the stratum
install/collect/cleanup and `validate_functional_rows` in `run_compile_step/4`
under phase `evaluator`; adds the metrics predicates (known-length install
counts, global table counters, `aggregate_all/3` per-ID cleanup leak count);
drops the dead hash-equating branch in `evaluation_lower/3`.

`v7/test/1_entrypoints.test.pl`: imports tracer helpers; adds
`evaluation_index_isolates_simultaneous_evaluation_ids`,
`evaluator_trace_is_gated_on_an_active_compile_trace`, and
`evaluator_trace_reports_stratum_metrics`.

`v7/src/2_comptime/1b_compiler_tracer.pl`: `compile_step_trace_on/0` gains the
`active_compile_trace(_)` guard; `finish_compile_step/4` hardened against a
failing metrics callback.

`v7/test/3_compiler_trace.test.pl`: adds the env-preserving `with_dl7_trace/2`
helper, `metrics_collected/2`, the throw/fail/malformed/value metrics goals, and
`step_metrics_fallback_covers_throw_fail_and_malformed`.

The cleanup metric uses `aggregate_all(count, evaluation_lower_index(EvaluationId, ...), LeftoverRowCount)`
in the current source; no `findall` remains in `evaluate_cleanup_metrics/3`.

No inherited file was modified.

## Status

Gate-3 tasks complete at the corrected revision, stopped for parent review. No
commit, push, merge, semantics change, or new cache/IR. Trace instrumentation is
opt-in and output-preserving.
