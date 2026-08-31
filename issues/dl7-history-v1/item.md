---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
size: M
epic: dl7-programmable-compiler
lane: dl7-programmable-compiler
lane_seq: 3
blocked_by: ['@dl7-generated-refreeze']
closed: 2026-08-30
closed_by: codex
commits:
- hash: 4c1e7f88a
  summary: Prove HistoryV1 generated behavior
---

# Prove HistoryV1 programmable compiler

## Description

Define HistoryV1 in the DL7 prelude. Intern source plus typed options, copy source edges, generate a relation definition and executable copy rule, and prove later-round execution in the consolidated oracle. Model class: medium.

## Acceptance Criteria

- [x] Source plus typed options determine one canonical specialization identity.
- [x] Source edges become generated result edges.
- [x] HistoryV1 emits a checked relation definition and executable rule.
- [x] The generated rule derives the authored source row after refreeze.
- [x] The runtime program contains the generated dependency and stratum.

## Tests Run

- [x] SWI suite: 15 of 15 passed.
- [x] Tree-sitter corpus: 1 of 1 passed.

## Implementation Notes

The proof intentionally implements copy behavior. Version, timestamp,
retention, append flow, and materialization remain later HistoryV1 rules.
Implemented in `0_types.dl7` with a consolidated oracle in
`1_entrypoints.test.pl`; commit `4c1e7f88a`.
