# DL7 compiler performance gate: nearest-shadow regression budget

Date: 2026-09-10
Lane: `feature-dl7-evaluator-trace-f41-20260910`
Base: `b900b80f2dc0c22f893bfab5963bca29c79b5d41`
Parent: `codex-2344`
Checkpoint under it: `0cf258711 perf(v7): index evaluator rows and trace scoped compiler work`

## TOC

- [Scope and ownership](#scope-and-ownership)
- [Profiles](#profiles)
- [Measured baselines](#measured-baselines)
- [Report format and failure semantics](#report-format-and-failure-semantics)
- [Diagnostic seam](#diagnostic-seam)
- [CLI self-check and unknown fixture](#cli-self-check-and-unknown-fixture)
- [Invocation contract and consumers](#invocation-contract-and-consumers)
- [Commands and actual results](#commands-and-actual-results)
- [Comparator tests](#comparator-tests)
- [CI integration proposal](#ci-integration-proposal)
- [Owned files and hashes](#owned-files-and-hashes)
- [Status](#status)

## Scope and ownership

Make compiler-performance regressions visible. The existing benchmark only
guarded `2_partial.dl7` (traced, 88M cold inference budget) and could not guard
`7_nearest_shadow.dl7`. This adds a fixture-selected profile:

- `2_partial` keeps its exact budgets, checkpoints, and traced
  (`DL7_TRACE=collect`) timed regime. It is retained **only as a legacy
  profile**; the 88M cold budget is not a claim about nearest-shadow and does
  not protect it.
- `7_nearest_shadow` gets an untraced timed regime: cold wall <= 3000 ms, cold
  inference budget 16,000,000, compiler rows 14,586, empty diagnostics, warm
  inference budget 5,000, and warm exact-output parity.

Owned files: `v7/bench/0_compiler_performance.pl`, `v7/justfile`,
`v7/test/20_compiler_performance.test.pl`. No workflow file was touched. No
kernel semantics change. No commit, push, or merge.

## Profiles

`fixture_profile(+Path, -Profile)` matches the **exact base name** via
`file_base_name/2` against `7_nearest_shadow.dl7` and `2_partial.dl7`. There is
no substring match, and an unknown path fails, so the CLI exits 2 with a named
error instead of silently borrowing another fixture's checkpoints.

| Field | `2_partial.dl7` (legacy) | `7_nearest_shadow.dl7` |
| --- | --- | --- |
| baseline version | `v7-compiler-perf/2_partial@2026-09-10` | `v7-compiler-perf/nearest-shadow@2026-09-10` |
| timed trace | `collect` | `off` |
| cold wall limit | none (wall reported) | 3000 ms |
| cold inference budget | 88,000,000 | 16,000,000 |
| warm inference budget | 50,000 | 5,000 |
| compiler rows | 15,542 | 14,586 |
| closure rounds | 8 (traced count retained) | `null` (unavailable when off) |
| diagnostics | cold/warm empty | cold/warm empty |
| warm output parity | yes | yes |

Checks are a list on the profile, so the comparator is data-driven and pure. The
comparator is inclusive: a value equal to the budget passes, budget+1 fails.

## Measured baselines

Nearest-shadow, fresh process, `clear_compiler_caches`, `DL7_TRACE` unset.
Budgets were pinned from these measurements, not fabricated.

| Regime | Metric | Observed | Budget |
| --- | --- | --- | --- |
| cold | inferences | 15,629,367 - 15,629,373 across runs | 16,000,000 |
| cold | wall ms | 2,558 - 4,545 across runs | 3,000 |
| cold | compiler rows | 14,586 | 14,586 |
| cold | diagnostics | `[]` | empty |
| cold | closure rounds | `null` (trace off) | not checked |
| warm | inferences | 2,144 - 2,148 | 5,000 |
| warm | wall ms | 21 - 108 | not checked |

The warm budget 5,000 is headroom over the measured 2,144/2,148 warm baseline.
Cold wall is load-sensitive: quiet `just compiler-perf-gate` runs land at
2.5-2.8 s, and a loaded run crossed at 4,545 ms. Cold inference carries only a
few-inference jitter with ~370k headroom, which is why the gate labels a
wall-only miss separately and does not call it a proven extra-work regression.

## Report format and failure semantics

`DL7-PERF-BEGIN fixture=... baseline=... timed_trace=...` prints before any
compile, so a `timeout` kill names the fixture. The JSON report and the stderr
lines carry, per regime, actual / budget / delta for wall and inferences plus
rows, closure rounds (`null` when the timed run is untraced), and the baseline
version.

Failure handling:

- Budgets are inclusive and pin-only. Nothing rewrites a baseline or bumps a
  budget.
- `performance_exit_code/2` maps an empty failure list to 0 and any failure to
  1; `main` halts with that code.
- Failures are classified so wall noise is not mistaken for extra work:
  `DL7-PERF-FAIL-CLASS wall_load_sensitive cold` for a cold-wall-only miss, and
  `DL7-PERF-FAIL-CLASS inference_regression cold|warm` for an inference miss.
- A single bounded diagnostic trace runs only after a failure, launches at most
  once, and cannot change the exit code. The exit code comes from the failures
  computed before the diagnostic.
- `timeout 20` firing is a nonzero exit, and the fixture is named on stderr
  first, so it is never a silent pass.

## Diagnostic seam

`diagnostic_outcome/2,3` runs one goal under `call_with_time_limit/3` and
returns an explicit status: `success`, `failure`, or `exception(Error)`. A
timeout surfaces as `exception(time_limit_exceeded)`. This makes the diagnostic
best-effort: an ordinary failure, a throw, or a timeout all return and the
original failure exit is preserved. It runs in `DL7_TRACE=steps` mode, so the
collected step rows are actually emitted to stderr and the CI log carries the
bottleneck evidence. The seam is unit-testable with injected goals and no
compile.

## CLI self-check and unknown fixture

`--self-check-pass`, `--self-check-wall`, and `--self-check-inference` inject one
known measurement set into the comparator and exit through the real CLI path:
no compile, no sleep. They exist so a subprocess can prove the actual exit code
and failure class, rather than only asserting the exit-code mapping in-process.

An unknown fixture path fails explicitly: the CLI prints
`DL7-PERF-ERROR unknown_fixture fixture=...` and exits 2.

## Invocation contract and consumers

`main/0` is an initialization-free export; the justfile runs it with
`-g main -t halt`. Repo-wide search for consumers of the benchmark
(`rg 0_compiler_performance`):

| Path | Invocation | Status |
| --- | --- | --- |
| `v7/justfile` | `swipl -q -s .../0_compiler_performance.pl -g main -t halt -- <fixture>` | updated, both recipes |
| `v7/README.md` | `cd v7 && just compiler-perf` (just recipe, no direct swipl) | no change needed |
| `v7/AGENTS.md`, receipts 9/10 | prose references | no change needed |

No in-base consumer invokes `swipl -s 0_compiler_performance.pl -- <path>`
without `-g main`. The parent wires direct `swipl` into the pinned CI container
and owns that reconciliation; no workflow was edited.

## Commands and actual results

One-command local gate (from `v7/`, new recipes under `timeout 20`):

```bash
cd v7
just compiler-perf-gate      # nearest-shadow untraced budget gate
just compiler-perf-test      # comparator, boundaries, CLI exit paths, no compile
just compiler-perf           # legacy 2_partial traced profile (unchanged, no timeout)
```

One actual gate run at this revision:

```text
just compiler-perf-gate -> PASS cold 2740 ms / 15,629,367 inf, warm 26 ms / 2,144 inf, rows 14586, closure_rounds=null, exit 0
```

Requested stdout/stderr examples (actual output):

Pass (`--self-check-pass`, exit 0):

```text
DL7-PERF-BEGIN fixture=7_nearest_shadow.dl7(self-check) baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF fixture=7_nearest_shadow.dl7 baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF cold wall_ms=1 budget=3000 delta=-2999 inferences=1 budget=16000000 delta=-15999999
DL7-PERF warm wall_ms=1 budget=null delta=null inferences=1 budget=5000 delta=-4999
DL7-PERF rows=14586 closure_rounds=null
```

Wall-only fail (`--self-check-wall`, exit 1):

```text
DL7-PERF-BEGIN fixture=7_nearest_shadow.dl7(self-check) baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF fixture=7_nearest_shadow.dl7 baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF cold wall_ms=9999 budget=3000 delta=6999 inferences=1 budget=16000000 delta=-15999999
DL7-PERF warm wall_ms=1 budget=null delta=null inferences=1 budget=5000 delta=-4999
DL7-PERF rows=14586 closure_rounds=null
DL7-PERF-FAIL cold_wall_budget(9999,3000)
DL7-PERF-FAIL-CLASS wall_load_sensitive cold
```

Inference fail (`--self-check-inference`, exit 1):

```text
DL7-PERF-BEGIN fixture=7_nearest_shadow.dl7(self-check) baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF fixture=7_nearest_shadow.dl7 baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
DL7-PERF cold wall_ms=1 budget=3000 delta=-2999 inferences=99999999 budget=16000000 delta=83999999
DL7-PERF warm wall_ms=1 budget=null delta=null inferences=1 budget=5000 delta=-4999
DL7-PERF rows=14586 closure_rounds=null
DL7-PERF-FAIL cold_inference_budget(99999999,16000000)
DL7-PERF-FAIL-CLASS inference_regression cold
```

Timeout naming (`timeout 2` on the real gate, exit 124):

```text
DL7-PERF-BEGIN fixture=7_nearest_shadow.dl7 baseline=v7-compiler-perf/nearest-shadow@2026-09-10 timed_trace=off
```

Unknown fixture (exit 2):

```text
DL7-PERF-ERROR unknown_fixture fixture=v7/test/fixtures/does_not_exist.dl7
```

Diagnostic evidence: injected failure -> `performance_exit_code=1`, one traced
compile ran after failure in steps mode (3,404 ms, 60 `COMPILE-TRACE-STEP` rows
in the log, `DL7-PERF-DIAGNOSTIC outcome=success`). The outcome unit tests cover
success, ordinary failure, throw, and timeout, all without a compile.

## Comparator tests

`v7/test/20_compiler_performance.test.pl`, 17/17 passing under `timeout 20`:

```text
nearest_shadow_cold_boundaries                 wall 3000 pass / 3001 fail; inf 16000000 pass / 16000001 fail
nearest_shadow_row_and_diagnostic_boundaries   14586 pass / 14587 fail; cold/warm diagnostics
nearest_shadow_warm_boundaries_and_output_parity  warm 5000 pass / 5001 fail; output mismatch
partial_boundaries                             88,000,000 / 50,000 / 15542 / 8 preserved
over_budget_measurements_exit_nonzero          injected over-budget -> exit code 1
within_budget_measurements_exit_zero           within budget -> exit code 0
over_budget_subprocess_exits_nonzero           comparator entrypoint in a real process exits 1
cli_self_check_pass_exits_zero                 real CLI, --self-check-pass exits 0, closure_rounds=null
cli_self_check_wall_fails_wall_only            real CLI exits 1, class wall_load_sensitive, no inference fail
cli_self_check_inference_fails_inference_only  real CLI exits 1, class inference_regression, no wall fail
unknown_fixture_profile_fails                  fixture_profile/2 fails for an unknown path
fixture_profile_requires_exact_basename        exact base name only; prefix/suffix paths fail
diagnostic_outcome_success                     diagnostic_outcome(true, success)
diagnostic_outcome_failure                     diagnostic_outcome(fail, failure)
diagnostic_outcome_throw                       diagnostic_outcome(throw(boom), exception(boom))
diagnostic_outcome_timeout                     diagnostic_outcome(0.02, (repeat, fail), exception(time_limit_exceeded))
unknown_fixture_cli_exits_two                  real CLI exits 2 and names the unknown fixture
```

The tests inject measurement terms and diagnostic goals only: no sleep, no large
compile. The subprocess tests resolve the bench path through the imported module,
so they work from any working directory.

## CI integration proposal

No workflow was edited. Proposed job for the workflow the parent maintains on
PR/push (adapt runner and install step to that file):

```yaml
  v7-compiler-perf:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
      - name: Install SWI-Prolog
        run: sudo apt-get update && sudo apt-get install -y swi-prolog
      - name: Performance budget comparator and CLI exit paths
        run: cd v7 && just compiler-perf-test
      - name: Nearest-shadow compiler regression gate
        run: cd v7 && just compiler-perf-gate
```

Risk to reconcile: the 3000 ms cold wall limit is tight on a shared runner. If
CI wall is noisy, either request a quieter runner or measure the CI baseline and
raise the wall limit with that evidence; do not raise it without a measured CI
number.

## Owned files and hashes

| File | Baseline sha256 | Owned sha256 |
| --- | --- | --- |
| `v7/bench/0_compiler_performance.pl` | `3ce87a3654ef972a0d6d6b6c6586f38024142bcc26333732f703fdbf74dbe77a` (inherited) | `2ca20da8797291d14a67bd3d3ffd182cec4abe211a49398f3fba63058fba3f7f` |
| `v7/justfile` | `d020ef28ec9ef71510038bba94143456a53b3821d62721a3030c180a14e8640d` (HEAD) | `98238fe359e30315600937f95ba29a84be90a0a25d548bac59a6e5128f19c8e5` |
| `v7/test/20_compiler_performance.test.pl` | new | `96972dd7010597296a6a4c36a19b4a2a79f7a3a4a4b5745c6d6c3bb12a3c5644` |

The checkpoint `0cf258711` committed the completed evaluator/tracer/reader/test/
receipt work; none of this gate's files were in it, and this task did not replay
or recommit that patch.

## Status

Corrections implemented and measured, stopped for immediate commit and merge. No
commit, push, or merge performed here. No workflow edit, no kernel semantics
change, no new watcher or framework, no broad suite.
