# clock checker finish brief (codex sol, no-commit flow)

Base sha stated at launch. Work ONLY inside your worktree. First action:
`git rev-parse HEAD` compared against the launch sha — READ-ONLY check; on
mismatch STOP AND REPORT. Do not commit, do not push; leave the tree dirty
for coordinator review (coordinator-cut worktree: git metadata writes fail).

## Context

The clock checker was paused mid-implementation at the v6.2 checkpoint.
Read chat_log/20260730.0.v6-2-ts-closeout.pl FIRST — specifically:
- paused_lane(clock_checker_full, ...) — module, registry dependency roles,
  historical replay receipts, compiler integration present but unfinished.
- decision(clock_checker_scope, label_ring_sign_grade_then_infer_clocks) —
  the RULED scope. Do not widen it.
- clock_checker_boundary(a2, not_provable, ...) — two zero-ring triggers
  intentionally implement either-source fire plus sampling; current facts
  cannot infer author batch-invariance. A2 STAYS not_provable. Do not try
  to prove it; make the checker state it as a named boundary.
- clock_checker_boundary(a6, runtime_comparison_required, ...).
- clock_checker_resume_order([...]) — follow it exactly:
  1. load and run v6/prolog/compile/test/3_clock_check.test.pl (and
     3_clock_history.pl) to see current red/green,
  2. finish the historical A2/A4/A5/A6/A7/A8/A9/A11 receipts,
  3. keep A2 as not_provable,
  4. compare A6 inferred offset facts to observed runtime ticks,
  5. run full plunit + conformance + sweep both modes.

The implementation lives in v6/prolog/compile/3_clock_check.pl (landed in
the checkpoint commit c6e2bf7b, 332 lines) with tests under
v6/prolog/compile/test/. TICK-MODEL.md holds the semiring/ring/sign/grade
semantics the checker labels. The A2..A11 letters are findings from the
language design review; their historical receipts replay recorded programs
through the checker.

## File ownership (yours alone)

v6/prolog/compile/3_clock_check.pl, v6/prolog/compile/test/3_clock_check.test.pl,
v6/prolog/compile/test/3_clock_history.pl, and v6/prolog/compile/compile.pl
for integration. registry.pl ONLY if dependency-role facts are genuinely
required. Nothing else: no tsv2, no fixtures outside clock fixtures, no
serve tests, no other compile modules.

## Hard laws

- Absolutely NO new surface syntax (locked surface_freeze +
  new_syntax_stop_rule: syntax pressure = STOP and report the fork with
  costs, never code).
- Named refusals over silent acceptance; the checker labels ring/sign/grade
  then infers clocks — refusal messages route through the existing
  0_refusal_messages.pl umbrella.
- Hermetic tool runs (SPREFA_CONFIG=/nonexistent/x.toml DL_NO_DAEMON=1,
  scratch dbs). Never touch ~/.local/state/sprefa or the daemon.
- swipl 10.0.2 GC abort under -g at large fixture counts is known;
  sweep.sh carries gc=false — do not remove it, and use the same flag if
  you hit "Mismatch in up phase".
- Line numbers in any doc are stale; re-find by symbol.
- Test budget: full battery at most 2 runs (end + one retry); focused
  plunit groups freely.

## Final summary shape

Per-receipt status for A2/A4/A5/A6/A7/A8/A9/A11 (proven / not_provable /
refused-with-name), battery counts (plunit, conformance, sweep both modes,
text-door), files changed, anything skipped with named reasons.
