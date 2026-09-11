# DL7 structured compiler debug trace

Base SHA `d9e8b6dbc7b39a1d810acba80e92251eb0f091fe`. Diagnostic instrumentation
only. The new `DL7_TRACE=debug` mode is opt-in; `steps`, `json`, `collect`, and
`off` keep their behavior, and trace-off compilation is byte-identical and within
the existing inference budgets.

## Contents

1. [Change](#1-change)
2. [Event schema](#2-event-schema)
3. [Instrumented boundaries](#3-instrumented-boundaries)
4. [Sampling and privacy](#4-sampling-and-privacy)
5. [Scope and lifecycle contract](#5-scope-and-lifecycle-contract)
6. [Example output](#6-example-output)
7. [Trace-off parity and performance](#7-trace-off-parity-and-performance)
8. [Tests and CI coverage](#8-tests-and-ci-coverage)
9. [Changed files](#9-changed-files)
10. [Method and scope](#10-method-and-scope)
11. [Correction: instrumentation gated by compile scope](#11-correction-instrumentation-gated-by-compile-scope)

## 1. Change

`v7/src/2_comptime/1b_compiler_tracer.pl` gains a fourth trace mode, `debug`,
alongside `steps`, `json`, and `collect`. The mode records structured
`COMPILE-TRACE-DEBUG` rows to stderr at compile finish, in sequence order. It uses
the existing `with_compile_trace/2` envelope, the existing compile-scope frames,
and new small helpers:

```text
debug_trace_on/0                   active compile trace AND mode == debug
debug_event(+Event, +Fields)       record one seq-ordered event, best effort
collected_debug_events/1           seq-ordered event list (exported for tests)
debug_histogram_fields/3           bounded structural key-count sample
debug_row_sample_fields/3          counts, row terms only under opt-in
debug_sample_limit/1               DL7_TRACE_SAMPLE_LIMIT, default 5
debug_rows_all/0                   DL7_TRACE_ROWS=all opt-in
```

`run_compile_phase/3` and `run_compile_step/4` gained a debug path that emits
begin/end events with `outcome` (`success`, `failure`, or `exception(Ball)`) and
the same wall/inference/gc/table metrics the phase and step writers report. The
existing step path is untouched, so no existing writer or test changed behavior.

Call sites in the compiler, checker, and evaluator emit additional events the
tracer cannot derive: round input/deltas, stratification input/result, per-stratum
install/collect/cleanup cardinalities, checker and lowerer input/output counts,
and diagnostics grouped by reason. Every metric is gathered only while
`debug_trace_on/0` holds; off-trace compilation computes none of it.

## 2. Event schema

One row per event. Prefix `COMPILE-TRACE-DEBUG`. Fields are a stable ordered
`name=value` list; the sequence number and current scope id are prepended.

| field | meaning |
| --- | --- |
| `seq` | trace-global monotonic sequence (shared with phase/step rows) |
| `event` | event name (see section 3) |
| `scope` | compile-scope frame id, or `none` outside a frame |
| `phase` | compiler phase or subsystem (`read`, `expand`, `lower`, `check`, `comptime`, `stratify`, `evaluator`, `memo`) when applicable |
| `step` | step name on `step_begin`/`step_end` |
| `round` | compiler round on `comptime_round` / `comptime_round_decision` |
| `stratum` | evaluator stratum level |
| `outcome` | `success`, `failure`, `exception(Ball)`, `continue`, `stable`, `limit_exhausted`, `installed`, `collected`, `cleared` |
| `wall_ms`, `inferences`, `gc_ms`, `tables` | measured on phase/step end events |
| metric names | step metrics are flattened onto `step_end` (for example `rows=3`, `cache_hit=0`) |

## 3. Instrumented boundaries

| area | events | where |
| --- | --- | --- |
| compile scope | `scope_enter` (`depth`), `scope_leave` | `open`/`close_compile_scope_frame` |
| phases | `phase_begin`, `phase_end` | `run_compile_phase/3` debug path |
| steps | `step_begin`, `step_end` | `run_compile_step/4` debug path |
| memo | `memo` with `action=hit/miss/store`, `kind`, `digest`, `key_size`, `value_shape`, `value_count` | `compile_scope_memo_lookup/store` |
| macrotime/comptime round | `comptime_round` (rules, seed_rows, frozen_edges, frozen_interns, generated_relations, generated_rules, closure_rows, `seed_relations`, `closure_relations`), `comptime_round_decision` (`continue`/`stable`/`limit_exhausted`) | `2_compiler.pl` |
| stratification | `stratification_input` (rules, dependencies, relations, positive, negative, gap_zero, gap_one), `stratification_result` (strata, diagnostics, strict_cycles, worklist_visits, level_changes, `strata_levels`) | `0_evaluator.pl` |
| evaluator strata | `evaluator_install` (`rule_relations`, `seed_relations`, `lower_relations`), `evaluator_collect` (`closure_relations` plus `global_table_answers`, `global_complete_calls`, `global_table_space_bytes`), `evaluator_cleanup` (erased_clauses, leftover_lower_rows) | `0_evaluator.pl` metrics callbacks |
| checker | `checker_input`, `checker_output`, `diagnostic_reasons` histogram | `1_checker.pl` |
| lowerer | `lowerer_input` (units), `lowerer_output` (nodes, edges, relations, seeds, rules, diagnostics, `diagnostic_reasons`) | `2_compiler.pl` |

`*_relations`, `strata_levels`, and `diagnostic_reasons` are bounded histograms:
`<label>_total`, `<label>_shown`, `<label>_omitted`, and the bounded
`<label>=[Key-Count, ...]` sample. Row deltas by relation are the paired
`seed_relations` and `closure_relations` histograms on the same `comptime_round`
event; frozen/generated counts are explicit fields.

## 4. Sampling and privacy

- `debug_histogram_fields/3` prints at most `DL7_TRACE_SAMPLE_LIMIT` structural
  key-count pairs (default 5). Keys are relation names, strata levels, or
  diagnostic reason functors, never row terms.
- `debug_row_sample_fields/3` reports `total`, bounded `shown`, and `omitted`.
  Row terms are `none` by default. `DL7_TRACE_ROWS=all` prints all rows because
  the operator explicitly opted in.
- Default debug output carries no source text, environment values, secrets, or
  arbitrary full literal text. Structural identifiers, integers, and hashes are
  the default payload.
- Every helper rejects a non-integer or negative `DL7_TRACE_SAMPLE_LIMIT` and
  falls back to the default window.

## 5. Scope and lifecycle contract

- `debug_event/2` returns immediately unless `debug_trace_on/0` holds, so a
  standalone `evaluate/4` or `stratify_rules/3` with `DL7_TRACE=debug` and no
  `with_compile_trace/2` records zero rows.
- `record_debug_event/2` is wrapped in `catch/3`; a throw in field derivation is
  swallowed and cannot change a memo lookup, a phase, or a step.
- `run_debug_phase/3` and `run_debug_step/4` record the outcome, then reproduce
  the original control flow: success returns true, failure fails, and an
  exception is rethrown as the original ball. `finish_compile_phase/3` records
  the phase row exactly as the non-debug path.
- `finish_compile_trace/3` writes debug rows (only in debug mode) then the
  existing summary and step writers, then `reset_compile_trace/0`, which now
  also clears `compile_debug_row/2`. `setup_call_cleanup/3` runs that cleanup on
  success, failure, and exception.
- Nested `with_compile_trace/2` calls push and pop scope frames; the memo store
  and debug rows stay per-frame/thread-local, so nested and concurrent
  compilations remain isolated.

## 6. Example output

`DL7_TRACE=debug`, `v7/test/fixtures/0_minimal.dl7`, excerpt:

```text
COMPILE-TRACE-DEBUG seq=0 event=scope_enter scope=1 depth=1
COMPILE-TRACE-DEBUG seq=11 event=scope_enter scope=2 depth=2
COMPILE-TRACE-DEBUG seq=12 event=lowerer_input scope=2 phase=lower units=1
COMPILE-TRACE-DEBUG seq=16 event=lowerer_output scope=2 phase=lower nodes=23 edges=37 relations=10 seeds=0 rules=12 diagnostics=0 diagnostic_reasons=[...]
COMPILE-TRACE-DEBUG seq=20 event=checker_input scope=2 phase=check nodes=23 pending_edges=37 relations=10 seeds=0 rules=12 origins=98
COMPILE-TRACE-DEBUG seq=21 event=memo scope=2 phase=memo action=miss kind=stratification digest=700789173 key_size=12 value_shape=miss value_count=0
COMPILE-TRACE-DEBUG seq=22 event=stratification_input scope=2 phase=stratify rules=12 dependencies=29 relations=12 positive=29 negative=0 gap_zero=29 gap_one=0
COMPILE-TRACE-DEBUG seq=23 event=stratification_result scope=2 phase=stratify strata=8 diagnostics=0 strict_cycles=0 worklist_visits=11 level_changes=0 strata_levels_total=1 levels_shown=1 levels_omitted=0 strata_levels=[0-8]
COMPILE-TRACE-DEBUG seq=24 event=memo scope=2 phase=memo action=store kind=stratification digest=700789173 key_size=12 value_shape=strata_diagnostics value_count=8
COMPILE-TRACE-DEBUG seq=25 event=checker_output scope=2 phase=check nodes=67 edges=82 relations=30 seeds=0 rules=12 diagnostics=0 diagnostic_reasons=[...]
```

The `7_nearest_shadow.dl7` debug run reaches `result(810,[])` with the same
program output as trace off. `2_partial.dl7` reaches `result(910,[])` and shows
rounds 2-6 `continue` then round 7 `stable`, with `generated_relations=1`,
`generated_rules=1` at the generation rounds.

## 7. Trace-off parity and performance

Exact output: `write_canonical/2` of `output(Rows, Runtime, Diagnostics)` for
`7_nearest_shadow.dl7`, fresh process, caches cleared.

| run | sha256 | bytes |
| --- | --- | --- |
| base (stashed change) trace off | `e244b85264abc6166adb60dc655e104ee0a2d6e911ac5c5258a2a803758fb33f` | 297,292 |
| traced trace off | `e244b85264abc6166adb60dc655e104ee0a2d6e911ac5c5258a2a803758fb33f` | 297,292 |
| traced `DL7_TRACE=debug` | `e244b85264abc6166adb60dc655e104ee0a2d6e911ac5c5258a2a803758fb33f` | 297,292 |

`cmp` identical for all three pairs.

Bench, `v7/bench/0_compiler_performance.pl`, `7_nearest_shadow.dl7`, trace off:

```text
DL7-PERF cold wall_ms=496 budget=3000 delta=-2504 inferences=3135832 budget=16000000 delta=-12864168
DL7-PERF warm wall_ms=3 budget=null delta=null inferences=2231 budget=5000 delta=-2769
DL7-PERF rows=810 closure_rounds=null
exit 0
```

Cold inferences are `3,135,832` (budget `16,000,000`, base `3,133,427` before
this change, `+2,405`). Warm inferences are `2,231` (budget `5,000`, base
`2,148`, `+83`). The delta is the off-path `debug_trace_on/0` checks in the
worklist and memo helpers; no budget moved.

## 8. Tests and CI coverage

`v7/test/3_compiler_trace.test.pl` gained twelve deterministic cases and the
tracer subprocess helper `debug_compile_probe/1`; three pre-existing cases are
unchanged.

| test | pinned result |
| --- | --- |
| `debug_mode_emits_structured_events_in_order` | exact event-name order `scope_enter, phase_begin, step_begin, step_end, phase_end, custom`; `step_end` `outcome=success`, `rows=3` |
| `debug_events_require_an_active_compile_trace` | env alone, no `with_compile_trace`, zero debug rows |
| `debug_step_outside_trace_runs_goal_but_skips_metrics_and_events` | `DL7_TRACE=debug`, no `with_compile_trace`: goal runs, counter-backed MetricsGoal called 0 times, 0 debug rows |
| `off_mode_records_no_debug_events` | off trace, collected events `[]` |
| `debug_row_samples_are_bounded_and_opt_in` | limit 2: `total=5 shown=2 omitted=3`, `rows=none`; `DL7_TRACE_ROWS=all`: `total=5 shown=5 omitted=0`, `rows=[a,b,c,d,e]` |
| `debug_histograms_are_bounded_and_opt_in` | limit 3: `total=5 shown=3 omitted=2`, 3 pairs; `all`: `total=5 shown=5 omitted=0`, 5 pairs |
| `debug_reports_memo_miss_store_and_hit` | repeated exact-rules stratification yields actions `[miss, store, hit]`, all `kind=stratification` |
| `debug_phase_events_preserve_failure_and_exception` | failure fails, exception rethrows; comptime outcomes `[failure, exception(debug_boom)]`; state cleared |
| `debug_exception_propagates_and_clears_state` | exception reaches caller unchanged; active trace 0, debug rows 0, memo stats 0 |
| `debug_nested_scopes_are_identified_and_isolated` | two distinct `scope_enter` ids; inner `scope_leave`; outer active afterward |
| `debug_trace_preserves_compile_output` | off vs debug `Rows/Runtime/Diagnostics` identical on `0_minimal.dl7` |
| `debug_compile_emits_all_instrumented_boundaries` | a real debug compile emits every instrumented event name incl. lowerer/checker/stratification/memo/round/evaluator |

Commands run, one `swipl` process at a time, each under `timeout 60` (every case
under 0.5 s):

| suite | result |
| --- | --- |
| `v7/test/3_compiler_trace.test.pl` | 15 pass (max 0.22 s) |
| `v7/test/18_binding_symmetry.test.pl` | 16 pass |
| `v7/test/19_lexical_binding.test.pl` | 10 pass |
| `v7/test/20_compiler_performance.test.pl` | 17 pass |
| `1_entrypoints` memo + evaluator trace subset | 12 pass |
| nearest-shadow perf gate (bench CLI) | exit 0, within budgets |

Coverage change: adds 12 tracer test cases and one subprocess compile-probe
helper. No test removed or changed. No performance budget moved or weakened. No
workflow file changed.

## 9. Changed files

| file | change |
| --- | --- |
| `v7/src/2_comptime/1b_compiler_tracer.pl` | `debug` mode, event store/writer, sample helpers, debug phase/step paths, memo/scope events |
| `v7/src/2_comptime/2_compiler.pl` | round input/decision events, lowerer input/output events and histograms |
| `v7/src/2_comptime/1_checker.pl` | checker input/output and reason-histogram events |
| `v7/src/1_libtime/0_evaluator.pl` | stratification input/result and worklist counters, evaluator install/collect/cleanup events |
| `v7/test/3_compiler_trace.test.pl` | eleven debug-trace cases, env/field helpers, compile probe |

No kernel relation, semantics, IR, emitter, V6, Rust, SQLite, or workflow change.
No new logging framework or dependency beyond `library(aggregate)` in the two
files that count relations/diagnostics.

## 10. Method and scope

- Debug capture: `DL7_TRACE=debug swipl -q -g "use_module('v7/src/2_comptime/2_compiler'), compile_dl7(...)" -t halt`.
- Exact output: `write_canonical/2` capture under `/tmp`, `cmp` plus `shasum`.
  The base term was captured with the change stashed in the same worktree path so
  the embedded `file(...)` term cannot differ.
- Bench: `swipl -q -s v7/bench/0_compiler_performance.pl -g main -t halt -- <fixture>`.
- Every command ran under `timeout 60` or less; one `swipl` process at a time.

## 11. Correction: instrumentation gated by compile scope

Two gates were too weak and did work outside an active compile trace. Both are
corrected in place; the schema, event set, and inside-scope behavior are
unchanged.

1. `run_compile_step/4` selected its debug wrapper with `compile_trace_mode(debug)`
   (`v7/src/2_comptime/1b_compiler_tracer.pl`). That only reads the mode, so
   `DL7_TRACE=debug` outside `with_compile_trace/2` still entered
   `run_debug_step/4`, called `MetricsGoal`, and emitted no rows but ran the
   metrics callback. The branch is now gated on `debug_trace_on/0`, which
   requires both an active compile trace and debug mode: outside a trace the
   Goal is called directly, `MetricsGoal` is never called, and no event is
   recorded. Inside a trace the two predicates agree, so traced behavior is
   identical.

2. `debug_reset_worklist_counters/0` and `debug_worklist_counters/2`
   (`v7/src/1_libtime/0_evaluator.pl`) unconditionally retracted and read the
   thread-local visit/change counters, so trace-off stratification still did
   counter retract/read work. Both now gate their state operations on
   `debug_trace_on/0`; trace off performs no counter retract, assert, or read.
   Debug-mode visit/change values are preserved: the counters are still reset
   before `calc_stratification_body/4` and read after it whenever the trace is
   active.

Fresh results, one `swipl` process at a time, each command capped at 20 s:

| check | result |
| --- | --- |
| `v7/test/3_compiler_trace.test.pl` | 15 pass (max 0.22 s) |
| nearest-shadow trace off, `0_compiler_performance.pl` | `rows=810`, diagnostics cold `[]` warm `[]` |
| nearest-shadow trace-off inferences | cold 3,136,024 / budget 16,000,000; warm 2,233 / budget 5,000 |
| nearest-shadow `DL7_TRACE=debug` | `event=stratification_result ... worklist_visits=11 level_changes=0` |
| `git diff --check` | clean (exit 0) |

No budget moved. Cold inferences are 3,136,024 against the base 3,135,832
(`+192`); the delta is the two added off-path `debug_trace_on/0` guard calls in
`calc_stratification/4`, the same category of check section 7 already records,
and no counter state is touched trace off. The new deterministic case
`debug_step_outside_trace_runs_goal_but_skips_metrics_and_events` uses a
`flag/3`-backed MetricsGoal and asserts `Calls == 0` and `DebugRows == 0`.
