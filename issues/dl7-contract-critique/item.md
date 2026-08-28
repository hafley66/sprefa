---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: opus
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-opus5]
size: S
lane: dl7-contract
lane_seq: 1
collision: [v7-design]
blocked_by: ['@dl7-kernel-contract']
---

# Critique DL7 kernel contract and delete unnecessary machinery

## Description

## Description

Critique the completed Sol kernel contract for internal contradictions and
unnecessary machinery. Read the plan, Sol report, and only the donor reports
needed to verify disputed claims.

## Acceptance Criteria

- [ ] Identify any duplicate representation of bind, edge, call, return, or
      specialization.
- [ ] Identify any evaluator branch that makes compile time and runtime
      mechanically different.
- [ ] Identify any feature outside the overnight ceiling.
- [ ] Propose deletions before additions.
- [ ] Write `v7/3_TASKS/results/1_CONTRACT_CRITIQUE.md`.
- [ ] Modify no implementation file.

## Tests Run

Run no suite.
