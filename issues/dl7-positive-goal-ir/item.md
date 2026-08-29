---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: codex
status: open
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-codex]
size: M
lane: dl7-datalog-kernel
lane_seq: 0
collision: [v7-datalog-check, v7-libtime, v7-test]
blocked_by: ['@dl7-compiler-split']
---

# Normalize positive checked goals

## Description

Wrap every resolved positive rule-body call in checked_goal(positive, Call) while preserving positive closure and dependency receipts.

## Type signatures

```prolog
checked_goal(+Polarity, +Call).
goal_call(+CheckedGoal, -Polarity, -Call).
goal_variables(+CheckedGoal, -VariableIdentities).
goal_dependency(+HeadRef, +CheckedGoal, -Dependency).
satisfy_goal(+EvaluationId, +CheckedGoal) is nondet.
```

For this slice, `Polarity = positive` only and
`Call = call(ref(RelationIdentity), Arguments)`.

## Instance timelines

Lowering retains pending positive calls. Checking resolves each relation
identity and wraps the resolved call. Checked goals live through dependency
derivation and one evaluator call. Evaluator-local native variables remain
proof-scoped and are cleaned with the existing evaluation namespace.

## Storage, reads, writes, uniqueness

The checked program stores one wrapper around each body call. Dependency rows
continue to derive one positive edge per head/body relation pair and collapse
as sets. Closure rows and runtime storage do not change.

## Acceptance Criteria

- [ ] Every checked positive body row is
  `checked_goal(positive, call(ref(Relation), Arguments))`.
- [ ] Bare checked body calls are absent.
- [ ] Dependency and variable analysis consume checked goals through explicit
  helper predicates.
- [ ] The evaluator dispatches checked positive goals through one helper.
- [ ] Existing Partial compiler closure remains term-identical.
- [ ] Existing dependency and stratum receipts remain term-identical.
- [ ] No negative syntax, strict stratum, aggregate, key, cons-mode, or emitter
  behavior lands in this task.

## Tests Run

- [ ] Existing consolidated V7 SWI suite passes with the normalized checked IR
  snapshot in the existing test file.
- [ ] Tree-sitter build gate passes unchanged.

## Implementation Notes

Contract source: `v7/design/3_DATALOG_EXTENSIONS.REVIEW.md`.
