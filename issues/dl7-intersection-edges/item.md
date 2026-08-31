---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-type-algebra
labels:
- size:large
size: L
lane: dl7-type-algebra
lane_seq: 7
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-intersection-apply']
---

# Merge ordered intersection edges

## Description

## Description

Emit left edges followed by right-only edges, deduplicate equal pairs, compute dense ordinals, and expose incompatible-label conflicts.

## Acceptance Criteria

- [ ] Compatible intersections produce one product.
- [ ] Duplicate edge pairs occur once.
- [ ] Conflicting targets derive intersection_conflict and no result edges.

## Tests Run

- [ ] Ordered merge, deduplication, and conflict receipts pass.
