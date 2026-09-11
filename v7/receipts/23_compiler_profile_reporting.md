status: done
next: parent (integration, fit and finish, commit only on explicit ask)
head: 312563dbd
scope: diagnostic profiling surface only; no compiler, graph, evaluator, rule,
       phase, IR, emitter, V6, Rust, or SQLite semantics; no budget moved.

# DL7 compiler profile reporting: wall + inference metrics

One bounded run now separates byte-stable structural/inference artifacts from
one run-specific timing artifact, and the HTML renders both metrics with the
cost center and numeric milliseconds visible without hover.

## Files

| file | change |
| --- | --- |
| `v7/bench/1_compiler_profile.pl` | `timing_dict/4`, `timing_span/3`, `wall_percent/3`, `cost_center_entry/3`, `max_wall_span/3`, `root_span/1`, `timing_json_text/2`; `report_stage/4` writes `5_timing.json` and passes it to `html_text/5`; `html_text/5` gains cost-center callout, span-metrics table, timing embed, and timing-aware renderer; `summary_text/6` gains wall-metric and duplicate labels |
| `v7/bench/2_compiler_flamechart.sh` | comment only: six artifacts |
| `v7/test/21_compiler_profile.test.pl` | 22 -> 25 cases: six-artifact lists, determinism scoped to `0/1/2/4`, timing contract, wall-isolation, HTML wall/inference view; lazy memoized run pair |
| `v7/receipts/23_compiler_profile_reporting.md` | this receipt |

## Invocation

```bash
./v7/bench/2_compiler_flamechart.sh <fixture> [output-directory]
# real report
bash v7/bench/2_compiler_flamechart.sh v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
# tests
perl -e 'alarm 20; exec @ARGV' -- swipl -q -s v7/test/21_compiler_profile.test.pl -g run_tests -t halt
```

One `swipl` process at a time, each under 20 s. No network, no CDN, no package
install.

## Artifact list

Default output `v7/out/compiler-profile/`, fixture `7_nearest_shadow.dl7`.

| file | bytes | kind | notes |
| --- | ---: | --- | --- |
| `0_profile.json` | 15,341 | deterministic | span tree + duplicate report, no wall |
| `1_folded.txt` | 2,226 | deterministic | inference self-weight folded stacks |
| `2_duplicates.tsv` | 3,670 | deterministic | per-category occurrence duplication |
| `3_flamechart.html` | 49,567 | run-specific | embeds `0_profile.json` + `5_timing.json` |
| `4_summary.txt` | 4,396 | deterministic | wall/duplicate metric labels, formula |
| `5_timing.json` | 9,072 | run-specific | total + per-span `wall_ms`/`wall_percent` |

`0_profile.json`, `1_folded.txt`, `2_duplicates.tsv` sha256 are byte-identical
to receipt 22 (`4d21dc39...`, `e095cf10...`, `ea862996...`), so the structural
and inference contracts are preserved. `4_summary.txt` changed only by added
labels and is byte-stable across fresh runs. Two fresh runs produced identical
`0/1/2/4`.

## Measured timing table

`7_nearest_shadow.dl7`, default run, `total_wall_ms=694` (fresh-process walls
vary run to run: 694 / 894 / 1081 ms observed).

| id | label | category | depth | inferences | wall_ms | wall_percent |
| --- | --- | --- | ---: | ---: | ---: | ---: |
| `compile` | compile | compile | 0 | 3,753,882 | 694 | 100.00 |
| `phase_82` | comptime | phase | 1 | 2,494,629 | 419 | 60.37 |
| `phase_73` | check | phase | 1 | 410,870 | 132 | 19.02 |
| `round_162` | round_2 | round | 2 | 818,350 | 130 | 18.73 |
| `round_89` | round_1 | round | 2 | 795,290 | 109 | 15.71 |
| `phase_5` | expand | phase | 1 | 565,853 | 71 | 10.23 |

Cost center (largest non-root span): `phase_82` `comptime`, 419 ms, 60.37%,
2,494,629 inferences. Duplicate figures unchanged from receipt 22: overall
19,599 / 23,945 = 81.85%.

## Metric definitions

| metric | definition | scope |
| --- | --- | --- |
| `inferences` | tracer-measured inference count per phase/step; stratum sums its install+collect+cleanup children | deterministic |
| `width` | same number, chart geometry and folded self-weight | deterministic |
| `wall_ms` | tracer-measured milliseconds per span; root = `total_wall_ms` | run-specific |
| `wall_percent` | `wall_ms / total_wall_ms * 100`, rounded to 2 decimals; 0.0 when total is 0 | run-specific |
| `duplicate occurrences` | repeated occurrences across recorded collections per category; `duplicate / total * 100` | deterministic |
| `duplicate CPU/inference percent` | `unavailable`; occurrences are not attributed to span inferences | n/a |

Wall never enters `0/1/2/4`; `5_timing.json` is the only machine-readable wall
artifact. HTML default view shows `Cost center: <label> (<id>) wall N ms (P%)`
and a span-metrics table with `wall_ms` and `wall_percent` columns; bar labels
repeat `<label> Nms P%`; the tooltip adds inferences and category.

## Browser mark count

Installed headless Chromium (no network):

```bash
~/Library/Caches/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell \
  --headless --disable-gpu --no-sandbox --virtual-time-budget=5000 \
  --dump-dom "file://$PWD/v7/out/compiler-profile/3_flamechart.html"
```

`Google Chrome for Testing 151.0.7922.34`, exit 0, no external refs in the
document. Rendered DOM: 72 `.span` chart marks (nonzero), 1 `cost-center` mark,
72 span-metric table rows. Generated page carries 72 spans.

## Tests

`21_compiler_profile.test.pl`: 25 cases, all pass, slowest
`all_six_artifacts_written` 1.927 s (below the 3 s case gate); shell cases
`shell_writes_six_artifacts` 0.978 s and `shell_report_failure_preserves_stage`
0.982 s; remaining cases at or below 0.117 s. Commands run one process at a
time under `alarm 20`.

New/changed pins: six-artifact existence; determinism over `0/1/2/4` only;
`timing_artifact_pins_total_and_per_span_wall` (run_specific, milliseconds,
root `wall_ms == total_wall_ms`, root `wall_percent == 100.0`, cost center is
the max-wall non-root span); `deterministic_artifacts_exclude_wall` (no
`wall_ms`/`total_wall_ms` in `0/1/2/4`); `html_shows_wall_and_inference_metrics_without_hover`
(timing embed, `timing-data`, `getElementById('timing-data')`, `#cost-center`,
`data-wall-ms`, `wall_percent`, `duplicate occurrences`,
`duplicate CPU/inference percent: unavailable`).

CI coverage: suite grows 22 -> 25 cases. No test removed. No budget moved.
Not committed.
