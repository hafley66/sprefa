---
created: 2026-08-30
updated: 2026-08-30
type: epic
status: done
priority: high
size: L
closed: 2026-08-30
closed_by: codex
commits:
- hash: 890e08ef3
  summary: Specify programmable compiler fragments
- hash: 4c1e7f88a
  summary: Prove HistoryV1 generated behavior
---

# DL7 programmable compiler fragments

## Description

Track ordinary DL7 compiler relations that generate checked relation definitions and executable rules across bounded compiler freeze rounds. Plan: v7/design/4_PROGRAMMABLE_COMPILER.PLAN.md.

## Acceptance Criteria

- [x] Generated definitions use ordinary compiler relation rows.
- [x] Generated rule heads and bodies use ordinary compiler relation rows.
- [x] Generated fragments enter the ordinary checked evaluator after refreeze.
- [x] HistoryV1 proves generated schema and executable behavior.

## Tests Run

- [x] SWI suite: 15 of 15 passed.
- [x] Tree-sitter corpus: 1 of 1 passed.

## Implementation Notes

Plan `890e08ef3`; generated-program carrier `67ab6c44b`; HistoryV1 proof
`4c1e7f88a`.
