---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, basement, model-glm53f]
size: M
lane: dl7-basement
lane_seq: 1
collision: [v7-datalog-lower]
blocked_by: ['@dl7-root-datums']
closed: 2026-08-28
closed_by: codex
commits:
- hash: 9eaeaa863
  summary: lower nested root forms
- hash: 06f82de74
  summary: consolidate basement passes
- hash: 7534695bc
  summary: derive containment from type edges
- hash: 5e8f3afe2
  summary: keep frontend fixtures equivalent
---

# Lower nested root forms to Datalog

## Description

Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`, milestone 2.

Lower canonical reader forms into a ground basement representation. Nested
colon binds become pending edges; products and sums become nested owners and
type nodes; facts and rules become reified Datalog calls. The binding edge
also supplies lexical containment when traversed in reverse.

## Acceptance Criteria

- [x] `lower_datalog/4` implements owner, reserve, and lower passes.
- [x] Nested `*` and `+` forms mint owners whose containing node is recoverable
  from their binding edge, without a duplicate parent-scope row.
- [x] Nested `:` forms retain owner, name, target term, and ordinal.
- [x] Atom, pending reference, variable, and constant terms stay distinct.
- [x] Every relation use requires an explicit bind.
- [x] Rules preserve authored goal order and reified variable identity.
- [x] No evaluator, type rule, application lowering, or test file is added.
- [x] `v7/3_TASKS/00_PROGRESS.md` records the receipt.

## Tests Run

- [x] One direct SWI lowering receipt from the worker brief.

## Implementation Notes

Worker brief: `v7/3_TASKS/13_DATALOG_LOWER.GLM53F.BRIEF.md`.

## Agent Runs

### 2026-08-29T00:42:14Z · @codex

Spawning GLM53F xhigh in feature/dl7-datalog-lower-glm after reviewed root-datum commits 0a477a098 and 392aa5521.

## Resolution

### 2026-08-29T01:12:11Z · @codex

Lowering now emits one ground basement graph. Type-node containment is derived from binding edges, all six consolidated SWI frontend tests pass, and no lowering test file was added.
