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
10. [Correction: path-stable, stage-preserving profile](#10-correction-path-stable-stage-preserving-profile)
11. [Boundaries and open items](#11-boundaries-and-open-items)

## 1. Command

```bash
./v7/bench/2_compiler_flamechart.sh <fixture> [output-directory]
```

The shell script is a non-interactive orchestrator: it resolves the fixture
and default output directory (exactly `<repo>/v7/out/compiler-profile`, anchored
at the repository root), then runs exactly one `swipl` process under
`timeout 20`. No network, package install, browser launch, or embedded analysis.
The Prolog process owns directory creation and prints the precise failure stage
(`source`, `usage`, `compile`, or `report`); the shell forwards the original
exit status unchanged and only adds `stage=timeout` when the 20-second cap
fires.

`v7/justfile` gains one recipe:

```bash
just compiler-flamechart v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
```

## 2. Artifacts

| file | content |
| --- | --- |
| `0_profile.json` | span tree plus duplicate-work report; no wall fields |
| `1_folded.txt` | `compile;phase;... <integer-weight>` folded stacks, self-weight in inferences, zero-weight spans omitted |
| `2_duplicates.tsv` | `category total unique duplicate percent top_repeated`, plus an `overall` row |
| `3_flamechart.html` | self-contained viewer; embeds `0_profile.json` verbatim in a `<script type="application/json">` block |
| `4_summary.txt` | human-readable denominator, formula, and per-category counts |

`7_nearest_shadow.dl7` produces 72 spans and a 15,341-byte JSON. Total wall time
is reported only to stderr as `DL7-PROFILE-WALL total_wall_ms=...`, so all five
files are byte-stable.

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
Wall is therefore never written into an artifact: the only wall value the
command emits is the `DL7-PROFILE-WALL total_wall_ms=...` line on stderr.
Because inferences are byte-stable, no separate logical-work width was needed.
Every displayed path is rendered against the repository root as `$REPO/...`
before it reaches JSON, TSV, summary, or HTML, so a different checkout prefix
produces identical bytes; the exact terms used for `==` never change.

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

Two fresh `swipl` processes, caches cleared, same fixture. All five files are
compared byte-for-byte with no filtered projection.

| artifact | bytes | sha256 (both runs equal) |
| --- | ---: | --- |
| `0_profile.json` | 15,341 | `4d21dc39aeac67943e6535863eac4984510442baaee27a0b4b6165101de58879` |
| `1_folded.txt` | 2,226 | `e095cf1026367e213aedf0bb51d72b0bd2dac78a7c9c177630f0e2ee49a12aea` |
| `2_duplicates.tsv` | 3,670 | `ea86299626de89759c3826e85dcedd5a8527a84e071290e444dabd8bde096959` |
| `3_flamechart.html` | 28,601 | `cd49932109b6c21c57943293cb7e99da526d264fecfb141698333c515379042a` |
| `4_summary.txt` | 4,296 | `0d417a9d97840f6ae62d5db7930aeecab460ce816afb082788d140cfc8da90f5` |

Compiler rows (810) and diagnostics (`[]`) are identical across runs. The
displayed fixture is `$REPO/v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`;
no artifact contains the absolute checkout path. The HTML test asserts the
embedded JSON equals `0_profile.json`, that every span parent exists, and that
the required labels are present.

## 7. Trace-off cost

`v7/bench/0_compiler_performance.pl`, `7_nearest_shadow.dl7`, trace off:

```text
DL7-PERF cold wall_ms=605 budget=3000 delta=-2395 inferences=3136311 budget=16000000 delta=-12863689
DL7-PERF warm wall_ms=4 budget=null delta=null inferences=2240 budget=5000 delta=-2760
DL7-PERF rows=810 closure_rounds=null
exit 0
```

Cold inferences are `3,136,311` against base `3,136,024` (`+287`); warm
`2,240` against base `2,233` (`+7`). The delta is the off-path
`profile_scope_on/0` guard calls. Every occurrence store, copy, sort,
canonicalization, and profiler object exists only while `with_profile_scope/1`
holds; trace off stores nothing.

`2_partial.dl7` runs `7323022 / 88000000` cold and fails only the
`compiler_row_checkpoint(910, 15562)` check. That mismatch is present at HEAD
without this change (verified by stashing the source edits); the fixture now
yields 910 compiler rows while the checkpoint still pins 15,562. No budget or
checkpoint was touched.

## 8. Tests and CI coverage

New `v7/test/21_compiler_profile.test.pl`, 22 cases, all pass (max case
`all_five_artifacts_written` 1.93 s, shell cases 0.93 s, rest under 0.05 s):

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
| `json_structure_has_stable_ids_and_existing_parents` | `span_count == len(spans)`; every parent id resolves; fixture starts `$REPO/`; no top_repeated identity contains the absolute root |
| `json_has_expected_hierarchy` | all seven categories; each stratum owns install/collect/cleanup |
| `all_five_artifacts_are_deterministic` | two fresh runs byte-identical for all five files |
| `rendered_paths_are_checkout_root_stable` | two synthetic roots render `$REPO/...` identically |
| `duplicate_tsv_names_columns_and_percent` | header columns; `overall` row |
| `summary_names_denominator_and_formula` | denominator, formula, `duplicate_inference_percent: unavailable` |
| `html_embeds_profile_json_and_labels` | embedded JSON equals the file; labels present |
| `unknown_source_exits_nonzero_with_stage` | exit 2, `stage=source` |
| `require_fixture_rejects_unknown` | semidet contract |
| `shell_writes_five_artifacts` | shell exit 0 and five files |
| `shell_unknown_source_preserves_stage` | shell exit 2, `stage=source`, not `stage=report` |
| `shell_report_failure_preserves_stage` | file-blocked output path: shell exit 4, `stage=report` |
| `shell_pins_default_output_directory` | script contains `${repo_root}/v7/out/compiler-profile` |
| `folded_omits_zero_weight_entries` | no folded line ends in ` 0` |

Commands run for this correction, each a fresh `swipl` under `timeout 20` or
less, one process at a time:

| suite | result |
| --- | --- |
| `21_compiler_profile.test.pl` | 22 pass (max 1.93 s) |
| `3_compiler_trace.test.pl` | 15 pass (max 0.25 s) |
| `./v7/bench/2_compiler_flamechart.sh <fixture>` (default and explicit dir) | exit 0, five byte-stable files |
| nearest-shadow perf gate (`0_compiler_performance.pl`) | exit 0, within budgets |

The base receipt additionally ran `18_binding_symmetry`, `19_lexical_binding`,
`20_compiler_performance`, `9_dbsp_plan`, `15_interned_storage`,
`16_storage_plain_field`, and `17_dl6_interned`; this correction touches no
predicate they exercise, and they were not rerun.

`14_sqlite_query_emitter.test.pl` fails its setup goal because
`sqlite_ivm/target/release/libsqlite_ivm.dylib` is not built in this checkout;
unrelated to this change and not newly broken.

Coverage change: the profile suite grows from 17 to 22 cases. No test removed.
No performance budget moved or weakened. No workflow file changed.

## 9. Changed files

| file | change |
| --- | --- |
| `v7/bench/1_compiler_profile.pl` | new: span builder, duplicate analyzer, five-file writer |
| `v7/bench/2_compiler_flamechart.sh` | new: bounded orchestrator |
| `v7/test/21_compiler_profile.test.pl` | new: 17 cases, then corrected to 22 |
| `v7/justfile` | one `compiler-flamechart` recipe |
| `v7/src/2_comptime/1b_compiler_tracer.pl` | profile scope, occurrence store, debug-row capture, profiler-silent debug writer |
| `v7/src/1_libtime/0_evaluator.pl` | occurrence recording for stratification, install rules/seeds/lower rows, closure rows |
| `v7/src/2_comptime/1_checker.pl` | occurrence recording for both checker entrypoints |

No cuts added. No declared det/semidet predicate was found to have an unwanted
choicepoint needing one.

## 10. Correction: path-stable, stage-preserving profile

Five review corrections, all diagnostic-surface only. No compiler, evaluator,
graph, or rule semantics changed.

1. **Default output path.** `2_compiler_flamechart.sh` defaulted to
   `<repo>/out/compiler-profile`; the contract is `<repo>/v7/out/compiler-profile`.
   It now uses the literal `${repo_root}/v7/out/compiler-profile`, and
   `1_compiler_profile.pl`'s own argument default anchors at the same path via
   `repository_root/1`.

2. **Stage preservation.** The shell appended `stage=report` to every nonzero
   status and ran its own `mkdir -p`. It no longer creates the directory and no
   longer labels a stage: the Prolog process owns
   `DL7-PROFILE-ERROR stage=<source|compile|report>`, and the shell forwards the
   original exit status. The shell still owns `stage=timeout`. Report-stage
   failures are now caught and labeled `report` rather than being mislabeled
   `compile` (`run_report_stage/5`).

3. **All five artifacts deterministic.** `0_profile.json` no longer contains
   `total_wall_ms` or `wall_observations`, so neither it nor the embedded HTML
   JSON carries wall. Wall is emitted only to stderr as
   `DL7-PROFILE-WALL total_wall_ms=...`. No sixth artifact was added. Two fresh
   processes byte-compare all five.

4. **Path-stable rendering.** The displayed fixture and every rendered
   `top_repeated` identity rewrite the checkout root to `$REPO/...`
   (`render_fixture_text/3`, `render_identity_text/3`,
   `normalize_rendered_paths/3`). Full exact terms are still stored and compared
   with `==`; normalization happens only at render time.
   `rendered_paths_are_checkout_root_stable` pins two synthetic roots to the
   same text.

5. **Zero-weight folded lines.** `emit_folded/1` omits weight-0 entries, so
   fully-attributed parent spans no longer appear as standalone folded stacks.

Correction verification (one `swipl` process at a time, each command under 20 s):

| check | result |
| --- | --- |
| `v7/test/21_compiler_profile.test.pl` | 22 pass (max 1.93 s) |
| `v7/test/3_compiler_trace.test.pl` | 15 pass (max 0.25 s) |
| `2_compiler_flamechart.sh <fixture>` default + explicit dir | exit 0; all five files byte-identical across fresh runs |
| nearest-shadow perf gate | cold 3,136,311 / budget 16,000,000; warm 2,240 / budget 5,000; exit 0 |
| artifact absolute-path scan | zero `/Users/...` occurrences in all five |

No budget moved. No compiler or evaluator semantics changed.

## 11. Boundaries and open items

- Instrumentation only; no semantic change. Trace-off output is unchanged.
- No dependency or logging framework beyond `library(http/json)`,
  `library(pairs)`, `library(filesex)`.
- `checker_input` stores whole basement terms; bounded rendering caps each
  printed identity at 160 characters, so the report stays small. Rendering
  rewrites the checkout root to `$REPO/...` after the 160-character cap is
  computed on the exact term.
- Tabled call identities and `duplicate_inference_percent` are `unavailable`
  and named as such in the report.
- `2_partial` compiler-row checkpoint is stale at HEAD; not adjusted here.
