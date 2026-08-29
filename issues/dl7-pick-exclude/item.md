---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: open
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f]
size: M
lane: dl7-kernel
lane_seq: 2
collision: [v7-kernel, v7-prelude]
blocked_by: ['@dl7-luna-review']
---

# Add userland Pick and Exclude goals

## Description

## Description

After the kernel review passes, add Pick and Exclude as `.dl7` standard-library
rules. Reuse the one oracle fixture and expected term.

## Acceptance Criteria

- [ ] Operator names occur only in prelude, fixture, and documentation.
- [ ] Pick uses a positive symbol-membership join.
- [ ] Exclude uses a completed lower-stratum anti-join.
- [ ] Both preserve relative order and assign dense output indices.
- [ ] The existing single oracle expands without adding a test.

## Test Run

Run the single SWI command once.

## Stop condition

Hail the parent if dense ranking requires an unplanned aggregate primitive.
