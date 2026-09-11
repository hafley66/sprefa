# DL7 deterministic compiler flamechart and duplicate-work report

Diagnostic instrumentation only. One command compiles a fixture through the
existing `with_compile_trace` envelope and writes a locally viewable
flamechart plus exact duplicate-work percentages. No kernel, graph, binding,
rule, phase, evaluator, IR, emitter, V6, Rust, SQLite, or workflow change. No
budget moved.

## Contents

1. [Command](#1-command)
2. [Artifacts](#2-artifacts)
3. [Span model and stable width](#3-span-model-and-stable-width)
4. [Duplicate-work contract](#4-duplicate-work-contract)
5. [Measured report](#5-measured-report)
6. [Determinism evidence](#6-determinism-evidence)
7. [Trace-off cost](#7-trace-off-cost)
8. [Tests and CI coverage](#8-tests-and-ci-coverage)
9. [Changed files](#9-changed-files)
10. [Boundaries and open items](#10-boundaries-and-open-items)

## 1. Command

```bash
./v7/bench/2_compiler_flamechart.sh <fixture> [output-directory]
```

The shell script is a non-interactive orchestrator: it resolves the fixture
and default output directory (`v7/out/compiler-profile`), then runs exactly one
`swipl` process under `timeout 20`. No network, package install, browser
launch, or embedded analysis. A nonzero exit names the failed stage
(`source`, `usage`, `compile`, `report`, or `timeout`).

`v7/justfile` gains one recipe:

```bash
just compiler-flamechart v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
```

## 2. Artifacts

| file | content |
| --- | --- |
| `0_profile.json` | span tree plus duplicate-work report; `wall_observations` and `total_wall_ms` are the only wall fields |
| `1_folded.txt` | `compile;phase;... <integer-weight>` folded stacks, self-weight in inferences |
| `2_duplicates.tsv` | `category total unique duplicate percent top_repeated`, plus an `overall` row |
| `3_flamechart.html` | self-contained viewer; embeds `0_profile.json` verbatim in a `<script type="application/json">` block |
| `4_summary.txt` | human-readable denominator, formula, and per-category counts |

`7_nearest_shadow.dl7` produces 72 spans and a 16,682-byte JSON.

## 3. Span model and stable width

Hierarchy:

```text
compile
  phase
    round            (evaluate_round(N) comptime step)
      stratum        (synthesised from install .. cleanup)
        install      (evaluate_install(N) step)
        collect      (evaluate_collect(N) step)
        cleanup      (evaluate_cleanup(N) step)
```

Phases, rounds, and install/collect/cleanup widths come from the `inferences`
field the tracer already measures on `phase_end` / `step_end`. A stratum span
sums its three children. Span identity is the tracer event sequence of its
opening event, so ids are stable and unique. `start` is the parent start plus
the running sum of preceding sibling widths, computed only from inferences.

Stable width metric: **inferences**. Two fresh processes on the same fixture
produced byte-identical inference rows (`read=3340`, `expand=565853`,
`total=...`), while `wall_ms` differed run to run (`read=3` vs `5` vs `9`).
Wall is therefore secondary metadata: it lives in the top-level
`wall_observations` map and `total_wall_ms`, both excluded from the
deterministic projection. Because inferences are byte-stable, no separate
logical-work width was needed.

`library(prolog_profile)` exposes per-predicate data through
`profile_procedure_data/2`, but its numbers are tick-sampled and not
byte-stable, so it is not used for the deterministic chart. The phase/step
spans supply the required hierarchy and widths.

## 4. Duplicate-work contract

Occurrences are recorded only inside `with_profile_scope/1`, at compiler
boundaries that already run:

| category | identity | boundary |
| --- | --- | --- |
| `stratification_input` | exact checked `Rules` term | `stratify_rules_with_dependencies/4` (recorded per request, memo hits included) |
| `checker_input` | `check_datalog(Basement, Origins)` or `check_resolved_rules(Relations, Rules)` | both checker entrypoints |
| `evaluator_installed_rules` | each installed `Rule` | `install_rules/3` |
| `evaluator_installed_seeds` | each installed `Seed` | `install_seeds/3` |
| `evaluator_installed_lower_rows` | each installed lower `Row` | `install_lower_rows/3` |
| `evaluator_collected_closure_rows` | each row of the sorted stratum closure | `collect_closure/2` |

Equality inside a category is decided by `==` after `term_hash/2` bucketing; a
shared hash never merges `==`-distinct terms. Categories are never merged:
`seed(Row)` and `closure(Row)` are different categories, and the same term
under `check_datalog` and `check_resolved_rules` stays in two identities.
`duplicate_groups_from_hashes/2` is exported so the test can force a hash
collision and prove the split.

Denominator: **sum of occurrence counts across categories**.
Formula: `duplicate_percent = duplicate_occurrences / total_occurrences * 100`.
`duplicate_inference_percent` is `unavailable`: occurrences are not attributed
to individual span inferences. `cross_stage_row_repetition_percent` is not
implemented and is omitted.

Tabled call identities are `unavailable`: SWI 10.0.2 exposes only aggregate
`table_statistics/2`, no public per-subgoal identity enumeration, and tapping
`proves/2` would change a tabled predicate.

## 5. Measured report

`7_nearest_shadow.dl7`, fresh process, caches cleared:

| category | total | unique | duplicate | percent |
| --- | ---: | ---: | ---: | ---: |
| `stratification_input` | 12 | 4 | 8 | 66.67 |
| `checker_input` | 9 | 5 | 4 | 44.44 |
| `evaluator_installed_rules` | 452 | 125 | 327 | 72.35 |
| `evaluator_installed_seeds` | 2,901 | 1,433 | 1,468 | 50.60 |
| `evaluator_installed_lower_rows` | 8,830 | 1,341 | 7,489 | 84.81 |
| `evaluator_collected_closure_rows` | 11,741 | 1,438 | 10,303 | 87.75 |
| **overall** | **23,945** | **4,346** | **19,599** | **81.85** |

Top repeated identity, `evaluator_collected_closure_rows`:
`15x call(ref(kernel(nil)),[const([])])`.

## 6. Determinism evidence

Two fresh `swipl` processes, caches cleared, same fixture.

| artifact | run 1 sha256 | run 2 | result |
| --- | --- | --- | --- |
| `1_folded.txt` | `790a22b418a1feac088bd00c9ce921ba0ae278ec4c54a561c2830aae61813fea` | same | byte-identical |
| `2_duplicates.tsv` | `d9242bde2f6e2c9d6b216f6ab558c48f3a085923211266aa3fa669fce9e276f4` | same | byte-identical |
| `4_summary.txt` | `71c74075e0a3044552ecc61aaef44bedab299d8af43673332c77b8cf54816e6c` | same | byte-identical |
| `0_profile.json` projection | `deterministic_profile_text/2` drops `wall_observations` and `total_wall_ms` | same | byte-identical |

Compiler rows (810) and diagnostics (`[]`) are identical across runs. The HTML
test asserts the embedded JSON equals `0_profile.json`, that every span parent
exists, and that the required labels are present.

## 7. Trace-off cost

`v7/bench/0_compiler_performance.pl`, `7_nearest_shadow.dl7`, trace off:

```text
DL7-PERF cold wall_ms=689 budget=3000 delta=-2311 inferences=3136308 budget=16000000 delta=-12863692
DL7-PERF warm wall_ms=5 budget=null delta=null inferences=2237 budget=5000 delta=-2763
DL7-PERF rows=810 closure_rounds=null
exit 0
```

Cold inferences are `3,136,308` against base `3,136,024` (`+284`); warm
`2,237` against base `2,233` (`+4`). The delta is the off-path
`profile_scope_on/0` guard calls. Every occurrence store, copy, sort,
canonicalization, and profiler object exists only while `with_profile_scope/1`
holds; trace off stores nothing.

`2_partial.dl7` runs `7323022 / 88000000` cold and fails only the
`compiler_row_checkpoint(910, 15562)` check. That mismatch is present at HEAD
without this change (verified by stashing the source edits); the fixture now
yields 910 compiler rows while the checkpoint still pins 15,562. No budget or
checkpoint was touched.

## 8. Tests and CI coverage

New `v7/test/21_compiler_profile.test.pl`, 17 cases, all pass (first
compile-bearing case 1.9 s, shell case 1.0 s, rest under 0.05 s):

| test | pinned result |
| --- | --- |
| `duplicate_groups_zero_repeats_and_order` | `[3-c, 2-a, 1-b]` |
| `duplicate_groups_all_unique_is_zero` | counts all 1 |
| `duplicate_groups_reject_hash_collision` | forced-equal hash, two `==`-distinct terms stay two groups |
| `duplicate_groups_merge_exact_identity` | `[2-a, 1-b]` |
| `duplicate_summary_separates_categories` | same term in two roles: each 1/1/0, 0.0 |
| `duplicate_summary_exact_percentages` | 3/2/1 -> 33.33; overall 4/3/1 -> 25.0 |
| `duplicate_summary_zero_when_all_unique` | overall 2/2/0, 0.0 |
| `all_five_artifacts_written` | five files exist |
| `json_structure_has_stable_ids_and_existing_parents` | `span_count == len(spans)`; every parent id resolves |
| `json_has_expected_hierarchy` | all seven categories; each stratum owns install/collect/cleanup |
| `folded_tsv_summary_and_json_are_deterministic` | two fresh runs byte-identical |
| `duplicate_tsv_names_columns_and_percent` | header columns; `overall` row |
| `summary_names_denominator_and_formula` | denominator, formula, `duplicate_inference_percent: unavailable` |
| `html_embeds_profile_json_and_labels` | embedded JSON equals the file; labels present |
| `unknown_source_exits_nonzero_with_stage` | exit 2, `stage=source` |
| `require_fixture_rejects_unknown` | semidet contract |
| `shell_writes_five_artifacts` | shell exit 0 and five files |

Existing suites, each a fresh process:

| suite | result |
| --- | --- |
| `3_compiler_trace.test.pl` | exit 0 |
| `18_binding_symmetry.test.pl` | exit 0 |
| `19_lexical_binding.test.pl` | exit 0 |
| `20_compiler_performance.test.pl` | exit 0 |
| `9_dbsp_plan.test.pl` | exit 0 |
| `15_interned_storage.test.pl` | exit 0 |
| `16_storage_plain_field.test.pl` | exit 0 |
| `17_dl6_interned.test.pl` | exit 0 |
| nearest-shadow perf gate | exit 0, within budgets |

`14_sqlite_query_emitter.test.pl` fails its setup goal because
`sqlite_ivm/target/release/libsqlite_ivm.dylib` is not built in this checkout;
unrelated to this change and not newly broken.

Coverage change: adds 17 profile test cases. No test removed or changed. No
performance budget moved or weakened. No workflow file changed.

## 9. Changed files

| file | change |
| --- | --- |
| `v7/bench/1_compiler_profile.pl` | new: span builder, duplicate analyzer, five-file writer |
| `v7/bench/2_compiler_flamechart.sh` | new: bounded orchestrator |
| `v7/test/21_compiler_profile.test.pl` | new: 17 cases |
| `v7/justfile` | one `compiler-flamechart` recipe |
| `v7/src/2_comptime/1b_compiler_tracer.pl` | profile scope, occurrence store, debug-row capture, profiler-silent debug writer |
| `v7/src/1_libtime/0_evaluator.pl` | occurrence recording for stratification, install rules/seeds/lower rows, closure rows |
| `v7/src/2_comptime/1_checker.pl` | occurrence recording for both checker entrypoints |

No cuts added. No declared det/semidet predicate was found to have an unwanted
choicepoint needing one.

## 10. Boundaries and open items

- Instrumentation only; no semantic change. Trace-off output is unchanged.
- No dependency or logging framework beyond `library(http/json)`,
  `library(pairs)`, `library(filesex)`.
- `checker_input` stores whole basement terms; bounded rendering caps each
  printed identity at 160 characters, so the report stays small.
- Tabled call identities and `duplicate_inference_percent` are `unavailable`
  and named as such in the report.
- `2_partial` compiler-row checkpoint is stale at HEAD; not adjusted here.
