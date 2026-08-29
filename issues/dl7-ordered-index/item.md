---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: in-progress
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-terra]
size: M
lane: dl7-datalog-kernel
lane_seq: 3
collision: [v7-datalog-check, v7-test]
blocked_by: ['@dl7-checked-foundation', '@dl7-datalog-rulings']
---

# Expose ordered edge indices relationally

## Description

Expose a checked ordering source used by userland dense-index rules.

## Type signatures

The selected contract is supplied by `@dl7-datalog-rulings`. The relational
candidate is:

```prolog
predecessor(+Owner, +EarlierIndex, +LaterIndex).
```

## Instance timelines

Ordering rows derive after ordered `':'/4` edges are checked and before
userland rule evaluation. Their transitive closure is an ordinary userland
relation.

## Storage, reads, writes, uniqueness

For the relational candidate, each adjacent checked edge pair emits one row.
The functional keys are `(Owner, LaterIndex)` and `(Owner, EarlierIndex)`.
No physical table or emitter row is introduced.

## Acceptance Criteria

- [ ] The selected ordering contract has one checked representation.
- [ ] Dense strict-before order can be derived using ordinary positive rules.
- [ ] Empty and singleton edge lists produce deterministic rows.
- [ ] Functional keys and row ordering have exact receipts.
- [ ] No Pick, Exclude, SQL, or emitter implementation lands in this task.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with one exact ordering receipt.
