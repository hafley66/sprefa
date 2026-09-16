# tsv2-live-extract-integration

Goal: integration tests where the REAL `sprefa-extract` binary runs as a host
through the real tsv2 pipeline. Today no test does: `hostDecode.test.ts:218-224`
proves the "sprefa_extract" executor by running `printf`, and
`6_host-extraction-batching.test.ts:191-236` stubs the executor to count calls.
Network-replay stays; these tests are additive.

## First action
`git merge --ff-only b1e909f471b4e382f23cb9bac3390a56159d2e3f`. Failure = STOP
AND REPORT. The Rust side landed as PR #235 on that sha; `--live-hosts` and
`v6/sprefa-engine-rs/tests/live_hosts.rs` exist now, your tsv2 tests stay additive.

## Files you own
- `v6/tsv2/tests/7_live-extract.integration.test.ts` (new)
- `v6/tsv2/tests/fixtures/live_extract/**` (new: 2-3 tiny committed source
  files, e.g. one `.rs` with two functions and one call between them)
- `v6/tsv2/package.json` ONLY to register the test file if the runner needs it.

FORBIDDEN: `v6/tsv2/serve/**`, `v6/tsv2/runtime/**`, `v6/sprefa-engine-rs/**`,
`v6/prolog/**`, every existing test file, `CLAUDE.md`. A second lane owns the
Rust side; touching its files is a defect.

## Read first (cite line numbers in your report)
1. `v6/tsv2/serve/1_hosts.ts`: `HostExecutors` (:261), `runShellLine`, decode
   shapes (:277 on), `HostRunner` (:498). Your test goes through these for
   real, zero stubs.
2. `v6/tsv2/tests/hostDecode.test.ts`: the serve-pipeline test harness already
   in that file (ports, `request`, `served.stop()`); copy its setup idiom.
3. A `.dl6` under `v6/dl/fixtures/` or a conformance fixture using `sh_decl`
   for the declaration spelling (e.g. `4_struct_values.pl:423-426` shape).
4. How the extract binary is invoked as a command line: grep
   `v6/prolog/compile/registry.pl:339-351` for the template shapes that route
   to `sprefa_extract`, and find one real emitted/served program that calls the
   extractor to copy its template spelling.

## The tests (fail-first; paste the pre-fix failing run in each header)
1. HAPPY PATH: serve a small `.dl6` whose host template invokes the real
   extractor over `tests/fixtures/live_extract/`; assert the extracted rows
   (function names, one call edge) land via the HTTP surface, exact rows, not
   row counts.
2. SABOTAGE: edit the fixture copy in a temp dir, rename a function, re-run,
   assert the changed row and ONLY the changed row differs.
3. CONTRACT: kill PATH so the binary is absent; assert the failure is a named
   host error naming the host and command, not a silent empty result.

Build the binary once in test setup: `cargo build --release -p sprefa-extract`
(or reuse an existing build script if the repo has one; cite it). Runtime
budget: the whole file under 10s per the repo law; 2-3 fixture files keep the
extractor fast.

## Validation (run, paste verbatim)
```bash
cd v6/tsv2 && node --test tests/7_live-extract.integration.test.ts   # or the repo's runner; cite it
cd v6/tsv2 && bash ../tools/../../v6/tools/green-parallel.sh 2>/dev/null || true  # do NOT gate on this; existing legs only
```
Also run the existing `tsv2-test` leg once and paste its tail to prove you
broke nothing.

## Style laws
- Async becomes rxjs above the SqlRunner seam in app code; TESTS may await the
  serve harness like the existing tests do; follow `hostDecode.test.ts` idiom.
- Descriptive variable names. No banned words (provenance, substrate,
  load-bearing, regime). Comment budget: constraints only.

## Report format
Zero-context coworker brief. Every claim path:line. Impossible step = STOP and
report the throw site.
