---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-type-algebra
labels:
- size:small
size: S
lane: dl7-type-algebra
lane_seq: 4
collision: [v7-test]
blocked_by: ['@dl7-structural-conformance']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Prove relation-valued contract edges

## Description

## Description

Add a fixture where a contract edge targets an ordinary relation type and prove it through structural conformance.

## Acceptance Criteria

- [x] Relation targets use ordinary type identities.
- [x] No relation-edge special case is added.

## Tests Run

- [x] Relation-valued edge receipt passes.
