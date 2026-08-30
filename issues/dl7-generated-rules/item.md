---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
size: L
epic: dl7-programmable-compiler
lane: dl7-programmable-compiler
lane_seq: 1
blocked_by: ['@dl7-generated-definitions']
closed: 2026-08-30
closed_by: codex
commits:
- hash: 67ab6c44b
  summary: Admit generated DL7 programs
- hash: 4c1e7f88a
  summary: Prove HistoryV1 generated behavior
---

# Admit generated rule heads and bodies

## Description

Add head/head_arg/body/body_arg carriers and assemble checked rule IR with dense positions, scoped variables, polarity, declarations, arity, modes, safety, and stratification. Model class: large.

## Acceptance Criteria

- [x] Head, body, and argument rows have explicit functional keys.
- [x] Rule and argument positions must be dense and zero-based.
- [x] Variables are scoped by generated rule identity.
- [x] Generated rules pass declaration, arity, mode, safety, and strata checks.
- [x] Orphan rule fragments diagnose.

## Tests Run

- [x] SWI suite: 15 of 15 passed.
- [x] Tree-sitter corpus: 1 of 1 passed.

## Implementation Notes

Carrier and checker implementation landed in `67ab6c44b`; orphan-fragment
validation landed in `4c1e7f88a`.
