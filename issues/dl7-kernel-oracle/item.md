---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: flash4
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-flash4]
size: S
lane: dl7-test
lane_seq: 0
collision: [v7-test]
blocked_by: ['@dl7-partial-goal']
---

# Pin the DL7 kernel with one exact oracle

## Description

## Description

Write one fixture and one test that proves reader output, compiler type closure,
normalized runtime rules, runtime reference closure, and cleanup determinism.

## Acceptance Criteria

- [ ] Exactly one focused test exists.
- [ ] One complete expected term covers every output listed above.
- [ ] The fixture is compiled twice in one SWI process.
- [ ] No existence-only or fragmented assertions.
- [ ] No V6 test, generated corpus, or engine suite runs.
- [ ] Test changes stay under `v7/5_TEST/`.

## Test Run

Run the single SWI command once and record exact pass/fail counts.

## Stop condition

Report production defects to the owning card. Do not patch production code.
