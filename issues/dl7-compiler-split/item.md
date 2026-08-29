---
created: 2026-08-29
updated: 2026-08-29
type: chore
assignee: luna
status: open
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, refactor, model-luna]
lane: dl7-refactor
lane_seq: 0
collision: [v7-datalog-lower, v7-datalog-check]
size: M
---

# Split DL7 lowering and checking stages

## Description

Pure-move split of v7/src/2_comptime/0_compiler.pl after the userland operator lane lands.

## Signatures

lower_datalog(+Unit, -Program, -Origins, -Diagnostics).
check_datalog(+Program, +Origins, -Checked, -Diagnostics).
compile_unit(+Unit, -Compiled, -Diagnostics).

## Instance lifetimes

Reader terms and origin rows live for one compilation. Basement data flows from lower_datalog/4 into check_datalog/4. Checked Datalog flows into compile_unit/3 and the shared evaluator. The refactor changes no term lifetime or identity.

## Storage, reads, writes, uniqueness

0_lowerer.pl reads one dl7_unit and produces the ground basement plus origin rows. 1_checker.pl reads that basement and produces checked_datalog with dependency and stratum rows. 2_compiler.pl composes reader, lowerer, checker, prelude, and evaluator. Edge keys remain (Owner, Name) and (Owner, Index). No persistent storage or runtime row changes.

## Body outline

1. Move lines 11 through 318 by predicate ownership into 0_lowerer.pl.
2. Move check_datalog/4 and its private predicates from line 327 onward into 1_checker.pl.
3. Rename 1_type_compiler.pl to 2_compiler.pl and update imports/call sites.
4. Preserve public signatures and exact output terms.
5. Mirror production names in existing tests without adding test files.

## Acceptance Criteria

- [ ] Production files follow dependency and reading order: 0_lowerer.pl, 1_checker.pl, 2_compiler.pl.
- [ ] Predicate bodies move without semantic edits.
- [ ] Existing seven SWI tests pass unchanged except import paths.
- [ ] Tree-sitter parser corpus remains green.
- [ ] No V6, Rust, TypeScript, prelude, fixture, or issue outside this card changes.
- [ ] Each production file stays below 500 nonblank, noncomment lines.

## Tests Run

- [ ] Focused seven-test SWI command.
- [ ] V7 just build.

## Implementation Notes

Pure-move card. Stop and report any circular module dependency or required semantic edit.
