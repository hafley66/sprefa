---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-type-algebra
labels:
- size:med
size: M
lane: dl7-type-algebra
lane_seq: 6
collision: [v7-prelude]
blocked_by: ['@dl7-prelude-files']
commits:
- hash: '1095987e5'
  summary: Draft DL7 userland type algebra
closed: 2026-08-31
---

# Add canonical intersection application

## Description

## Description

Declare Intersect(Left, Right, return) and intern its ordered arguments with reversible full relation rules.

## Acceptance Criteria

- [x] One canonical result identity exists per ordered pair.
- [x] Application uses the shared intern and cons relations.

## Tests Run

- [x] Exact application identity receipt passes.
