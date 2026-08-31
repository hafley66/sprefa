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
lane_seq: 1
collision: [v7-prelude]
blocked_by: ['@dl7-prelude-files']
commits:
- hash: '1095987e5'
  summary: Draft DL7 userland type algebra
closed: 2026-08-31
---

# Add canonical conformance application

## Description

## Description

Declare Conforms(Source, Contract, return) and intern its ordered arguments with forward and reverse rules.

## Acceptance Criteria

- [x] One canonical proof identity exists per source-contract pair.
- [x] Full relation calls retain reverse lookup.

## Tests Run

- [x] Exact application identity receipt passes.
