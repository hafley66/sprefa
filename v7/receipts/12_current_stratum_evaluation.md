# Current-stratum evaluator scheduling

## Status

Blocked by an observable compiler semantic change. The approved
`PlainRules = CurrentRules - AggregateRules` candidate was applied and tested,
then removed after the canonical nearest-shadow gate failed. Shipping evaluator
behavior remains unchanged.

- Base SHA: `0ed06ee176ec50fbd549d7810e1878eb059444b8`
- Final source SHA: base evaluator restored; no commit created
- Final changed files:
  - `v7/test/1_entrypoints.test.pl`
  - `v7/receipts/12_current_stratum_evaluation.md`
- Inspected and restored: `v7/src/1_libtime/0_evaluator.pl`
- Next: parent

## Candidate and failure

The candidate replaced `rule_through_level/3` selection with current-stratum
selection. Its trace installed `31,11,39,3,3,26,2` rules instead of
`31,42,81,84,87,113,115` across levels 0 through 6.

Per seven-stratum evaluator invocation:

- baseline assertions: `553`
- candidate assertions: `115`
- exact repeated lower-rule reduction: `438`
- candidate remaining repeated lower-rule assertions: `0`
- restored shipping remaining repeated lower-rule assertions: `438`

Nearest-shadow stopped in compiler round 1:

```text
before  wall_ms=2638  inferences=15,629,373  rows=14,586  diagnostics=[]
candidate wall_ms=883  inferences=6,482,876   rows=0
candidate diagnostics=[missing_derived_bind(module(file(...)),'Name',0)]
```

The last candidate stratum closure contained `15,108` rows. The baseline
contains `15,110` at the corresponding point. The candidate did not reach an
equivalent successful second compiler round, so no successful whole-compile
assertion reduction is claimed.

## Cause

Completed `LowerRows` are the answers returned by the stratum's general
`proves(EvaluationId, Call)` collection. They do not contain every answer that
a later, more instantiated call can produce.

The nearest-shadow source contains `(: Name (Option text))`. The lower
`Option/2` constructor rule reaches `kernel(cons)` and `kernel(intern)`. During
its lower-stratum general call, `Source` is unbound and `cons_construct/3`
requires a ground head and tail, so no `Option(text, Result)` row is collected.
At the later `:/4` stratum, the edge rule binds `Source` to `text`. Shipping
behavior reinstalls `Option/2`; the bound call then creates
`application(Option,[text])` and records the intern request. Current-only rule
installation removes that demand path, producing `missing_derived_bind`.

The added positive cross-stratum test pins this mode-sensitive path directly.
Its complete closure includes the upper result and `kernel(intern)` request;
the demand-specific lower answer is not a member of the general collected
closure under the current variant-tabled evaluator.

## Exact-output gate

Canonical capture used `timeout 20 swipl -q -g "use_module('v7/src/2_comptime/2_compiler'),use_module('v7/src/2_comptime/1c_compiler_cacher'),clear_compiler_caches,statistics(inferences,I0),get_time(T0),compile_dl7('v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7',Rows,Runtime,Diagnostics),get_time(T1),statistics(inferences,I1),Wall is round((T1-T0)*1000),Inf is I1-I0,length(Rows,N),setup_call_cleanup(open(OUT,write,S,[encoding(utf8)]),(write_canonical(S,output(Rows,Runtime,Diagnostics)),write(S,'.'),nl(S)),close(S)),format('capture wall_ms=~d inferences=~d rows=~d diagnostics=~q~n',[Wall,Inf,N,Diagnostics])" -t halt`, substituting each named `/private/tmp` output path for `OUT`.

```text
before    c1dd35044a4b9a1b7543ceda9ba1265df35055920957defb11a43796c4a761b5
candidate 0884603dec5d02c89d32ccab8f6bf25f6771a1671dbe7609ee39eddec2eda0d5
restored  c1dd35044a4b9a1b7543ceda9ba1265df35055920957defb11a43796c4a761b5
before vs candidate: DIFFERENT
before vs restored: IDENTICAL_BYTES
restored rows=14586 diagnostics=[]
```

The supplied Terra SHA `d4a364...` was captured from the parent checkout.
Canonical rows contain absolute source paths, so this lane's byte hash differs.
Before/candidate/restored comparisons above all use this lane path.

## Semantic tests

Five direct evaluator cases were added. Each asserts the complete sorted closure
including `kernel(nil)` and exact empty diagnostics:

1. demand-bound positive cross-stratum dependency through `cons` and `intern`
2. strict negation over completed lower rows
3. count aggregate over a completed lower relation
4. same-stratum transitive recursion
5. lower facts mixed with current-stratum recursion

```text
timeout 20 swipl -q -s v7/test/1_entrypoints.test.pl \
  -g "run_tests(dl7_entrypoints:[evaluator_current_stratum_reads_completed_positive_dependency,evaluator_current_stratum_reads_completed_strict_negation,evaluator_current_stratum_counts_completed_lower_relation,evaluator_current_stratum_keeps_transitive_recursion,evaluator_current_stratum_mixes_lower_facts_with_recursion])" -t halt
result: 5/5 passed with the restored evaluator
```

The bounded fixture corpus comparison stopped at its first difference,
`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Tests 15, 18, 19 and
`2_partial.dl7` were therefore not compared under the invalid candidate.

## Validation

```text
timeout 20 swipl -q -s v7/test/1_entrypoints.test.pl -g "run_tests(dl7_entrypoints:[stratification_is_pure_deterministic_and_strict_cycle_checked,evaluation_index_enumerates_free_row_and_relation,evaluation_index_matches_partial_and_ground_arguments,evaluation_index_retains_arity_zero_and_extra_arguments,evaluation_index_exact_unification_rejects_forced_collision,evaluation_index_is_cleared_across_lifecycle_paths,evaluation_index_isolates_simultaneous_evaluation_ids,evaluator_trace_is_gated_on_an_active_compile_trace,evaluator_trace_reports_stratum_metrics,prefix_negation_is_safe_stratified_and_cleanup_scoped,count_groups_completed_lower_proofs_and_rejects_bad_placement])" -t halt: 11/11 passed
timeout 20 swipl -q -s v7/test/3_compiler_trace.test.pl -g run_tests -t halt: 3/3 passed
timeout 20 swipl -q -s v7/test/20_compiler_performance.test.pl -g run_tests -t halt: 17/17 passed
timeout 20 just -f v7/justfile compiler-perf-gate: passed
  cold wall_ms=2622 inferences=15,629,367 rows=14,586 diagnostics=[]
  warm wall_ms=23 inferences=2,144 diagnostics=[]
git diff --check: passed
```

Nearest-shadow is below 3 seconds after restoration, so no remaining-top-five
profile was run. No budget, workflow, cache, phase, type, or lower-row lifetime
change was made.

CI coverage change: five evaluator cases added; zero changed or removed.
