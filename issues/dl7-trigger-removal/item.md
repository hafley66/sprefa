---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: S
lane: dl7-expression-flow
lane_seq: 5
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-uniform-expressions']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 5d79f6f73
  summary: Drive Partial from relation application
---

# Remove generic trigger scaffolding

## Description

Delete partial_request and related trigger names from the prelude, fixture, tests, and compiler evidence after application-driven construction lands. Model class: Flash4.

## Acceptance Criteria

- [x] `partial_request` has zero V7 occurrences.
- [x] Partial, Pick, Exclude, and HistoryV1 still reach stable closure.
- [x] Fixture wording uses applications and ordinary relation rows.

## Tests Run

- [x] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 6. This is a mechanical deletion after bind applications land.

## Resolution

### 2026-08-30T23:39:08Z · @codex-0

partial_request has zero V7 code occurrences. Partial, Pick, Exclude, and HistoryV1 reach stable closure. Complete V7 SWI passed 20 of 20 and Tree-sitter passed 1 of 1.
