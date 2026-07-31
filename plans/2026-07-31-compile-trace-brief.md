# Compile trace: always-on time attribution (luna brief, 2026-07-31)

Base sha: be092b2e. Worktree: ../sprefa-codex-comptrace, branch codex/compile-trace.
NO-COMMIT flow: leave the tree dirty, coordinator reviews and commits.

## Why
`just compile-speed` caught a +1888% parse regression but only named the PHASE.
The predicate-level answer took a human running `execution_profile_dl6`
(v6/prolog/compile/6_profile.pl:168) by hand — it had never been run. The user
directive: full tracing, always on; nobody should ever ask "where is the time
going" again. BENCH-TRACE (v6/bench-cli) is the precedent: always-on wall
accounting printed every run.

## Files you own (touch NOTHING else)
- v6/prolog/compile/compile.pl (trace hook only)
- v6/prolog/compile/6_profile.pl
- v6/prolog/compile/scripts/1_compile_speed.sh
- v6/prolog/compile/compile_dl6.sh
- v6/justfile (one new recipe only)

Another lane concurrently owns v6/prolog/compile/parse_dl.pl and
scripts/compile-speed-baseline.tsv — do not touch either.

## Tasks (in order)
1. **Always-on phase trace.** Every `compile_dl6/2` and `compile_program/6`
   entry prints exactly ONE line to stderr on completion:
   `COMPILE-TRACE program=<name> parse=<ms>/<inf> plan=<ms>/<inf> lower=<ms>/<inf> boot=<ms>/<inf> emit=<ms>/<inf> write=<ms>/<inf> total=<ms>/<inf>`
   (wall ms, SWI inference deltas per phase; reuse the phase measurement shape
   already in 6_profile.pl — factor it so the always-on path and the
   DL_PERF_LOG JSONL path share ONE measurement implementation, no fork).
   stderr only; stdout contracts and compiled output bytes must be unchanged.
   The DL_PERF_LOG opt-in JSONL path keeps working exactly as before.
2. **Gate names the predicate.** In 1_compile_speed.sh: when the ratchet
   reports REGRESSION, automatically rerun the single worst-regressed
   program+phase under `compile_profile:execution_profile_dl6/2` and print the
   top 15 self-time lines into the gate output, clearly labelled. Cap the
   profile run via v6/tools/run-capped.sh with a 120s budget (timeout-gun law:
   nothing runs uncapped). Gate exit code semantics unchanged.
3. **Recipe.** `just compile-profile program='<name>'` in v6/justfile: runs
   execution_profile_dl6 on v6/dl/fixtures/<name>.dl6, capped at 120s.

## Laws
- Comments: only constraints the code cannot show. No narrative headers, no
  change-log comments, no dates/arc references. One-line purpose max.
- No new deps. No em dashes in any text you write.
- Timeout gun: every long-running command you invoke goes through
  v6/tools/run-capped.sh.
- Descriptive variable names, never single letters.

## Receipts (run all, paste outputs in your final report)
- `cd v6/prolog/compile && bash /…/v6/tools/run-capped.sh 180 bash scripts/1_compile_speed.sh`
  — currently FAILS with 4 parse regressions (pre-existing, another lane is
  fixing it). Your change must make this failing run ALSO print the top-15
  profile lines naming parse_dl predicates ($skip_list / mark_furthest area).
  Do not fix the regression; do not touch the baseline.
- One compile via compile_dl6.sh of door-handwritten.dl6: COMPILE-TRACE line
  appears on stderr, output .ts byte-identical to a pre-change compile
  (sha256 both, show the shas).
- Same compile with DL_PERF_LOG set: JSONL lines still written, shape
  unchanged (show one line).
- `just compile-profile program=door-handwritten` produces a profile table.
- swipl plunit for compile/test/ still passes.

## Report shape
Base sha verified, per-task what changed (file:symbol), each receipt's output,
any deviation stated loudly. STOP AND REPORT on any blocked command; never
work around a denial.
