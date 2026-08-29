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
lane_seq: 1
collision: [v7-datalog-lower]
blocked_by: ['@dl7-root-datums']
---

# Lower nested root forms to Datalog

## Description

Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`, milestone 2.

Lower canonical reader forms into a ground basement representation. Nested
colon binds become pending edges; products and sums become nested owners and
scopes; facts and rules become reified Datalog calls.

## Acceptance Criteria

- [ ] `lower_datalog/4` implements owner, reserve, and lower passes.
- [ ] Nested `*` and `+` forms mint owners with one parent-scope row.
- [ ] Nested `:` forms retain owner, name, target term, and ordinal.
- [ ] Atom, pending reference, variable, and constant terms stay distinct.
- [ ] Every relation use requires an explicit bind.
- [ ] Rules preserve authored goal order and reified variable identity.
- [ ] No evaluator, type rule, application lowering, or test file is added.
- [ ] `v7/3_TASKS/00_PROGRESS.md` records the receipt.

## Tests Run

- [ ] One direct SWI lowering receipt from the worker brief.

## Implementation Notes

Worker brief: `v7/3_TASKS/13_DATALOG_LOWER.GLM53F.BRIEF.md`.
