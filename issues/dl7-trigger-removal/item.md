---
created: 2026-08-30
updated: 2026-08-30
type: task
status: open
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: S
lane: dl7-expression-flow
lane_seq: 5
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-uniform-expressions']
---

# Remove generic trigger scaffolding

## Description

Delete partial_request and related trigger names from the prelude, fixture, tests, and compiler evidence after application-driven construction lands. Model class: Flash4.

## Acceptance Criteria

- [ ] `partial_request` has zero V7 occurrences.
- [ ] Partial, Pick, Exclude, and HistoryV1 still reach stable closure.
- [ ] Fixture wording uses applications and ordinary relation rows.

## Tests Run

- [ ] Complete V7 SWI and Tree-sitter gates pass.

## Implementation Notes

Plan milestone 6. This is a mechanical deletion after bind applications land.
