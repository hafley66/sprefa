---
created: 2026-08-29
updated: 2026-08-29
type: chore
assignee: luna
status: done
closed: 2026-08-29
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

- [x] Production files follow dependency and reading order: 0_lowerer.pl, 1_checker.pl, 2_compiler.pl.
- [x] Predicate bodies move without semantic edits.
- [x] Existing seven SWI tests pass unchanged except import paths.
- [x] Tree-sitter parser corpus remains green.
- [x] No V6, Rust, TypeScript, prelude, fixture, or issue outside this card changes.
- [x] Each production file stays below 500 nonblank, noncomment lines.

## Tests Run

- [x] `swipl -q -g "load_files(['v7/test/0_reader.test.pl','v7/test/1_entrypoints.test.pl'],[silent(true)]),run_tests,halt"` (7 passed, 0 failed).
- [x] `cd v7 && just build` (1/1 passed).

## Implementation Notes

Pure-move card. Stop and report any circular module dependency or required semantic edit.
