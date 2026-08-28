---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: flash4
status: open
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, model-flash4]
size: S
lane: dl7-test
lane_seq: 1
collision: [v7-test, v7-engine-contract]
blocked_by: ['@dl7-engine-seam', '@dl7-kernel-oracle']
---

# Smoke one V7 artifact through the existing Rust engine

## Description

## Description

Implement one smoke artifact only if the Terra seam report identifies an
existing engine command requiring zero Rust changes.

## Acceptance Criteria

- [ ] Zero files under `v6/sprefa-engine-rs` change.
- [ ] Zero Rust files are added under V7.
- [ ] Existing engine command consumes one V7-generated temporary artifact.
- [ ] Output is compared exactly.
- [ ] No additional test file is created when the kernel oracle can host the
      command.

## Test Run

Run one exact engine command once. Run no suite.

## Stop condition

Record the blocker and add no workaround when engine or TS code would change.
