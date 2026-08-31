---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-type-algebra
labels:
- size:large
size: L
lane: dl7-type-algebra
lane_seq: 7
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-intersection-apply']
commits:
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Merge ordered intersection edges

## Description

## Description

Emit left edges followed by right-only edges, deduplicate equal pairs, compute dense ordinals, and expose incompatible-label conflicts.

## Acceptance Criteria

- [x] Compatible intersections produce one product.
- [x] Duplicate edge pairs occur once.
- [x] Conflicting targets derive intersection_conflict and no result edges.

## Tests Run

- [x] Ordered merge, deduplication, and conflict receipts pass.
