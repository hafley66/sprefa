---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: codex
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-flash4, model-codex]
size: S
lane: dl7-test
lane_seq: 0
collision: [v7-test]
blocked_by: ['@dl7-partial-goal']
closed: 2026-08-29
commits:
- hash: f3479f5c1
  summary: pin comptime with one vertical oracle
---

# Pin the DL7 vertical kernel with one consolidated oracle

## Description

Extend the existing entrypoint test module with one vertical test for the
Partial fixture. Keep the expected result as one normalized term so absolute
source paths and semantic IDs can vary while graph meaning remains exact.

## Acceptance Criteria

- [x] One focused Partial test is added to the existing entrypoint test module.
- [x] One expected term covers compiler diagnostics and row count.
- [x] The same term covers generated Partial node, classifier, labels, targets, and indices.
- [x] The same term covers checked runtime graph and program counts plus normalized call shapes.
- [x] The same term proves evaluator temporary clauses are empty after compilation.
- [x] The fixture compiles twice in one SWI process with identical compiler and runtime terms.
- [x] No existence-only assertion or additional test file is added.
- [x] No V6, Rust, TypeScript, generated corpus, or engine suite runs.

## Tests Run

- [x] One focused SWI command passes all 7 consolidated V7 tests.

## Implementation Notes

The test lives in `v7/test/1_entrypoints.test.pl` and reuses
`v7/test/fixtures/2_partial.dl7`.

## Resolution

### 2026-08-29T04:15:36Z · @issuectl

The consolidated Partial oracle passes with six existing reader and entrypoint tests: 7 passed, 0 failed, and no choicepoint warning.
