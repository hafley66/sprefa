---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, basement, model-glm53f]
size: M
lane: dl7-basement
lane_seq: 2
collision: [v7-datalog-check]
blocked_by: ['@dl7-datalog-lower']
---

# Resolve and check the Datalog basement

## Description

Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`, milestone 3.

Resolve pending names through nested owner scopes, emit canonical colon edges,
validate the positive Datalog program, and emit dependency plus SCC stratum
rows. This card ends the first basement slice immediately before evaluation.

## Acceptance Criteria

- [ ] `check_datalog/4` resolves local and parent-scope names.
- [ ] `int`, `text`, `any`, and `type` resolve through the primitive root.
- [ ] Resolved binds are canonical `':'(Owner, Name, Target, Index)` rows.
- [ ] Resolved calls carry `ref(Target)`, `var(Identity)`, and `const(Value)`.
- [ ] Explicit relation, arity, ground-seed, and positive-rule safety checks run.
- [ ] Distinct positive dependency rows and one SCC stratum row per relation emit.
- [ ] Nested product and sum, recursive, undeclared, arity, and unsafe receipts run.
- [ ] No evaluator, negation, aggregate, interning behavior, or test file is added.
- [ ] `v7/3_TASKS/00_PROGRESS.md` records the receipt.

## Tests Run

- [ ] One direct SWI checker receipt from the worker brief.

## Implementation Notes

Worker brief: `v7/3_TASKS/14_DATALOG_CHECKS.GLM53F.BRIEF.md`.
