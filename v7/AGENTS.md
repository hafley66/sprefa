# V7 Agent Notes

V7-local gates for compiler work. The repository root `AGENTS.md` still applies.
Reusable SWI-Prolog engine technique (tabling, memo scoping, C-builtin pricing)
lives in the SWI-Prolog skill, not here; this file holds only gates that are
specific to compiling DL7 in `v7/`.

## Performance gates

- **Individual test case under 3 seconds.** A `run_tests` case must report under
  3 s. Shared `setup/1` compile cost is separate, but keep it bounded too. A
  focused file that needs hundreds of seconds is a bug, not a budget.
- **Never start a 300-second compiler test.** If a compile is not done in tens
  of seconds, stop and profile instead of waiting.
- **Start with `DL7_TRACE=steps`.** It prints `COMPILE-TRACE-STEP` rows with
  `wall_ms`, `inferences`, `gc_ms`, and `tables` per phase/step, plus the
  `COMPILE-TRACE` phase summary. Cap probe runs with `timeout` (15 s for
  profiling probes).
- **Inspect inferences and table reuse per V7 step.** A step with millions of
  inferences for a small input is a repeated traversal. `tables=0` on a step
  that logically uses a scoped table means the table is torn down before the
  snapshot, so it cannot show reuse; count calls with `library(prolog_profile)`
  instead.
- **Scope and reset memo state per compile.** Memo/table state must not outlive
  a compile, a round, or a unit. Use `setup_call_cleanup/3` (or `thread_local`
  plus an explicit reset) at the owning boundary so cold and warm runs stay
  byte-stable.
- **Measure, do not guess.** `v7/bench/0_compiler_performance.pl` is the
  deterministic inference gate (cold/warm inference budgets plus compiler-row
  and closure-round checkpoints). Run it before and after a compiler performance
  change; wall-clock alone is not evidence. Do not raise a budget to make a
  change pass; a checkpoint may move only when the change alters the compiled
  program, and the receipt must cite the measured reason.
