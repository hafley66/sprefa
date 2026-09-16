# V7 Agent Notes

V7-local rules for compiler work. The repository root `AGENTS.md` still applies.

## Performance footguns

- **Individual test case under 3 seconds.** A `run_tests` case must report under
  3 s. Shared `setup/1` compile cost is separate, but keep it bounded too. A
  focused file that needs hundreds of seconds is a bug, not a budget.
- **Never start a 300-second compiler test.** If a compile is not done in tens
  of seconds, stop and profile instead of waiting.
- **Start with `DL7_TRACE=steps`.** It prints `COMPILE-TRACE-STEP` rows with
  `wall_ms`, `inferences`, `gc_ms`, and `tables` per phase/step, plus the
  `COMPILE-TRACE` phase summary. Cap probe runs with `timeout` (15 s for
  profiling probes).
- **Inspect inferences and table reuse.** A step with millions of inferences for
  a small input is a repeated traversal. `tables=0` on a step that logically
  uses a scoped table means the table is torn down before the snapshot, so it
  cannot show reuse; count calls with `library(prolog_profile)` instead.
- **Table pure recursive graph walks.** Positive recursive closures belong behind
  `:- table` scoped to one evaluation namespace, with
  `abolish_table_subgoals/1` at the boundary. See
  `../v6/prolog/0_compiler_relations.pl` (`compiler_proves/2` keyed by `EvalId`)
  and `../v6/prolog/analyze.pl` (`body_ref_uses/2` plus `reset_body_use_cache/0`).
- **Hoist repeated relation and list scans.** Building an index once beats
  scanning a relation per element. See `../v6/prolog/0_generic_expand/4_type_views.pl`
  (`type_row_memo/3`, hashed key plus `=@=` guard) and the `b2f36b92b` and
  `ba920f52e` fixes in `v6/` history: index occurrences once, replace
  accumulating-list scans with a keyed lookup.
- **Scope and reset tables or memos per compile.** Memo state must not outlive a
  unit, a round, or a compile. Use `setup_call_cleanup/3` (or `thread_local` plus
  an explicit reset) at the owning boundary so diagnostics and outputs stay
  byte-stable across cold and warm runs.
- **Measure, do not guess.** `v7/bench/0_compiler_performance.pl` is the
  deterministic inference gate (cold/warm inference budgets plus compiler-row and
  closure-round checkpoints). Run it before and after a compiler performance
  change; wall-clock alone is not evidence.
