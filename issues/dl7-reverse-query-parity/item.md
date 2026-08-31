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
lane_seq: 6
collision: [v7-test]
blocked_by: ['@dl7-trigger-removal']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: 0dd3d92d4
  summary: Prove reverse queries through full DL7 calls
---

# Prove full-tuple reverse query parity

## Description

Add exact tests showing a known generic result can bind its source through the unchanged full relation tuple. Model class: Flash4.

## Acceptance Criteria

- [x] One explicit full-tuple query binds source from known result.
- [x] The query uses the same constructor relation as forward expression use.
- [x] Expression lowering does not rewrite explicit full-arity calls.

## Tests Run

- [x] Focused V7 SWI test passes.

## Implementation Notes

Plan milestone 7. This is the executable receipt for retained Prolog symmetry.

## Resolution

### 2026-08-30T23:42:28Z · @codex-0

A complete Partial(Source, Result) call recovers Source from a known derived result. The focused receipt passed in 0.08 seconds.
