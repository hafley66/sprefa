# write-verb-interface PAUSED

Paused 2026-08-20 on Chris's word (perf work takes the machine). Steps 5 and the
verb contract are CODE COMPLETE on `feature/write-verb-interface` (base
`3993e44aa`); receipts are in `TASKS/write-verb-interface.REPORT.md`.

Green here: conformance (1 known red), `just plunit` (8 red, the same 8 names as
origin/main), whole-corpus stage-1 byte identity (0 changed files), both
shared-frontier parity gates 8/8 with the oracle as a third arm, `cargo test
--lib` 26/26.

Not run: `cd v6/tsv2 && npm test`. It was started twice and killed twice (the
first run had the tree swapped under it by the plunit baseline checkout, the
second by this pause). `gen_emitted/` already holds 352 modules, so it needs no
sweep in front of it.

NEXT ACTION: run `cd v6/tsv2 && npm test`, paste its tail into the report's
`### node tests` section, then open the PR (title
"feat(frontier): write-verb interface, retraction parity (steps 5 + contract)",
superseding #378).

BLOCKING, not this lane's to fix: `65607a8d5` reverted PR #372's IR-version work
on origin/main, so the pre-commit rail returns 400 for everyone and three Rust
test targets do not compile. Every commit here carries `-n` and says so. Needs
its own card.
