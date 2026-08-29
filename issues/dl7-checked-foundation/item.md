---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: done
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-terra]
size: L
lane: dl7-datalog-kernel
lane_seq: 1
collision: [v7-datalog-lower, v7-datalog-check, v7-libtime, v7-test]
blocked_by: ['@dl7-datalog-rulings', '@dl7-positive-goal-ir']
closed: 2026-08-29
closed_by: codex
commits:
- hash: 568f71038
  summary: Restore checked relation keys
- hash: bca56c573
  summary: Check constructive goal modes in authored order
- hash: fbaf13db1
  summary: Derive checked dependency strata
- hash: 2ffba59c2
  summary: Record the checked foundation receipts
---

# Restore checked Datalog foundation contracts

## Description

Normalize checked goals, authored-order safety, relation keys, closure key validation, and pure stratification while preserving positive closure.

## Type signatures

```prolog
checked_goal(+Polarity, +Call).
relation(ref(RelationIdentity), +Arity, +KeySets).
check_goal_sequence(+Goals, +Bound0, -Bound, -Diagnostics).
stratify_rules(+Rules, -DerivedStrata, -Diagnostics).
validate_functional_rows(+Relations, +Rows, -Diagnostics).
```

## Instance timelines

Lowering erases reader syntax into pending calls. Checking resolves every body
item into `checked_goal(positive|negative, call(ref(...), Arguments))`.
Authored-order safety and strata are computed before evaluator state is
installed. Key validation runs on the final sorted closure.

## Storage, reads, writes, uniqueness

`relation/3` owns arity and zero-based key-position sets. Positive-only closure
output must remain exact. `':'/4` carries keys `(Owner, Name)` and
`(Owner, Index)`. Evaluator-local clauses and tables are cleaned after each
evaluation.

## Acceptance Criteria

- [x] Every checked rule body item uses `checked_goal/2`.
- [x] Authored-order safety covers ordinary calls and the selected `cons/3`
  and `intern/3` modes.
- [x] Relation rows carry key sets and final closure rejects conflicting rows.
- [x] One pure stratification routine emits deterministic positive-only
  stratum-zero receipts and diagnoses strict cycles.
- [x] Existing positive Partial closure remains term-identical.
- [x] No negation execution, aggregate folding, ordering source, or emitter
  code lands in this task.

## Tests Run

- [x] Existing consolidated V7 SWI suite with exact foundation receipts in the
  existing test file.

## Implementation Notes

Full contract and collision audit:
`v7/design/3_DATALOG_EXTENSIONS.REVIEW.md`.

## Resolution

### 2026-08-29T22:15:52Z · @codex

Checked goal IR, relation key metadata, final-closure functional validation, authored-order constructive modes, and pure deterministic stratification are implemented. Focused SWI suite: 10/10. Tree-sitter: 1/1. issuectl doctor reports only the pre-existing repository findings.
