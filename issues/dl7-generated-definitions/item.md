---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
size: M
epic: dl7-programmable-compiler
lane: dl7-programmable-compiler
lane_seq: 0
closed: 2026-08-30
closed_by: codex
commits:
- hash: 67ab6c44b
  summary: Admit generated DL7 programs
- hash: 4c1e7f88a
  summary: Prove HistoryV1 generated behavior
---

# Admit generated relation definitions

## Description

Add def(Relation, Arity) as a keyed kernel carrier. Assemble canonical checked relation rows, reject invalid arities and conflicting identities, and expose the definition on the next compiler round. Model class: medium.

## Acceptance Criteria

- [x] `def/2` is a keyed kernel relation with type-graph signature rows.
- [x] Dense nonnegative arities assemble into checked relation declarations.
- [x] Invalid arities, conflicting definitions, and base identity collisions diagnose.

## Tests Run

- [x] SWI suite: 15 of 15 passed.
- [x] Tree-sitter corpus: 1 of 1 passed.

## Implementation Notes

Implemented in `0_lowerer.pl`, `1_checker.pl`, and
`1a_generated_program_assembler.pl`. Commit `67ab6c44b`; collision receipt in
`4c1e7f88a`.
