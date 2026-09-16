# RESUME: mdquery — ownership amended, proceed

Your STOP.md was right; the coordinator verified `memberchk(Lang, [rust, ts,
tsx, js, go, kotlin])` at `v6/prolog/0_program_check.pl:245` and confirms no
other lane owns that file.

Amendment to BRIEF.md, everything else unchanged:

- Option 1 from your STOP.md is taken: you now ALSO own the one-line whitelist
  edit at `v6/prolog/0_program_check.pl:245` (add `md`, `html`).
- `v6/prolog/0_ast_expand.pl` needs no edit if, as your receipts say, it holds
  no list; state that in the report instead of editing it.
- Your plunit receipt (`:2934`/`:2940` pin `plain` and an anonymous variable,
  no membership pin) is accepted; no plunit edit needed unless a test fails.

Proceed with the full original brief: candidate table first, then the rust
work in v6/sprefa-extract/, validation commands verbatim, commit on this
branch, REPORT-MDQUERY.md at worktree root. Delete STOP.md in the same
commit. If anything else deviates, STOP again the same way.
