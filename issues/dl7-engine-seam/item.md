---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: terra
status: open
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, model-terra]
size: M
lane: dl7-engine
lane_seq: 0
collision: [v7-engine-contract]
blocked_by: ['@dl7-luna-review']
---

# Pin the DL7 to existing Rust engine seam

## Description

## Description

Trace the smallest adapter from the normalized V7 runtime program to the
existing ProgramJson and Rust engine door. This is a contract task first.

## Acceptance Criteria

- [ ] Name exact existing Rust type, loader, command, and `ir_version` source.
- [ ] Name the minimum ProgramJson fields for one inert or one-row program.
- [ ] Confirm zero engine source changes are required, or record the blocker.
- [ ] Define an adapter signature without importing V6 parser terms.
- [ ] Write `v7/3_TASKS/results/9_ENGINE_SEAM.md`.

## Tests Run

Read-only command inspection. Run no suite.
