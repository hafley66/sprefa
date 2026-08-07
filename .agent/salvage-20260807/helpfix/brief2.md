# Lane: FINISH the extract --help rewrite (pass 1 of 2)

You are pass 1 of 2; a coordinator design-review follows. Favor plain code and
exact compliance. If reality deviates from this brief, STOP and report in
REPORT.md; do not improvise.

## Situation
A previous agent started this exact task and died mid-edit. The worktree
`/Users/chrishafley/projects/sprefa-lanes/helpfix` (branch
lane/help-family-unfuck @ 173d308c) has UNCOMMITTED changes to the one owned
file: `v6/sprefa-extract/src/bin/extract/help.rs`. The full contract with the
EXACT replacement text is `brief.md` at the worktree root. Do not re-plan the
text; brief.md's text is final.

## Task
1. `cd /Users/chrishafley/projects/sprefa-lanes/helpfix`
2. Read `brief.md` in full.
3. Compare the current `v6/sprefa-extract/src/bin/extract/help.rs` against
   brief.md's specified `LONG_ABOUT` and `FAMILY_LONG` text. The edit may be
   partially applied, fully applied, or corrupted mid-hunk.
4. Make help.rs match brief.md exactly: the two constants replaced with the
   brief's text, every other constant, clap attribute, and flag untouched.
   Mind brief.md's escaping note (`\"diet\"`, the `= "\` opening, the
   trailing `";`).
5. You own exactly ONE file. `git status` must show only help.rs modified.

## Gates (from brief.md, run all, paste output into REPORT.md)
- `cargo build --release --features cli --bin extract` in v6/sprefa-extract: clean.
- `./target/release/extract --help | head -40` renders the new text.
- `cargo test --features cli` in v6/sprefa-extract: all pass (the help test
  `the_cli_help_names_the_fallback_formats` in tests/6_document_formats.rs
  greps the rendered help; the LANGUAGE COVERAGE table lines must survive).
- `git diff --stat` shows exactly one file changed.

## Commit and report
Commit on lane/help-family-unfuck, message:
`extract: --help speaks plain words, family defined at first use`
The pre-commit rail needs the extract binary you just built. Never push, no
subagents, no npm/cargo dependency changes. REPORT.md at worktree root: gate
outputs pasted verbatim, deviations list (expected: none).
