# Comment bloat sweep (luna brief, 2026-07-31)

NO-COMMIT flow: leave the tree dirty, coordinator reviews and commits.
You work alone: no subagents, no parallel helpers, sequential file edits.

## Directive
Comment budget law (CLAUDE.md style notes, user-set 2026-07-31): comments
state only constraints the code cannot show. The codebase is full of
change-log essays written by past agents. Strip them.

## KILL on sight
- change-log/arc narrative: "FIXED 2026-...", "landed in the X arc",
  "was A now B", "superseded by", dates, merge shas, lane/agent names.
- comments restating what the next line does.
- justification-to-reviewer prose ("this is correct because", "note that
  we now", ruling explanations that belong in plans/ or rulings.pl).
- multi-paragraph module headers narrating history or design debate.
  A module keeps at most a 1-3 line purpose statement plus genuine
  constraint notes.
- commented-out dead code.

## KEEP (never delete)
- constraint comments the code cannot show: invariants, orderings, units,
  why a guard exists, known hazards ("X does not terminate under Y",
  "driver exposes no Z API").
- @-markers with a scanner behind them (@eprintln-ok, @recompute
  unguarded, any other: grep the marker across scripts/tests first; if
  anything greps for it, it stays).
- sabotage/fail-first receipts in TEST file headers (standing repo law).
  Trim surrounding narrative, keep the receipt.
- shebangs, directives (:- module, pragmas), anything a script parses
  (grep the exact phrase before deleting structured-looking comments).
- When unsure: KEEP and list it in your report's kept-unsure table.

## Out of scope everywhere
All *.md files; chat_log/; plans/; docs/; v6/prolog/ARCH.pl;
v6/prolog/conformance/rulings.pl; v6/prolog/conformance/fixtures/**;
any gen_emitted/generated file; v6/prolog/compile/parse_dl.pl;
v6/prolog/compile/scripts/compile-speed-baseline.tsv; v5 rust src/**;
justfiles.

## Receipts (run via v6/tools/run-capped.sh with stated budgets, paste)
Per the scope named in your launch prompt:
- prolog scope: conformance (budget 300s, expect 281 PASS), plunit
  (expect 269), sweep both modes with counts identical to a pre-change
  run you record FIRST, TEXT_DOOR counts unchanged, roundtrip.
- TS scope: tsv2 test suite, import gate, dl npm test, store npm test.
- Final: git diff --stat vs base; spot-check 3 changed files showing
  every removed line was a comment or blank line.

## Method
One package at a time. Run that package's receipt before moving to the
next so a breakage is localized. Do not ADD comments. No em dashes.
Deviations reported loudly; blocked command = STOP AND REPORT.
