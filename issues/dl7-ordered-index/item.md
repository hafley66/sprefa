---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: done
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-terra]
size: M
lane: dl7-datalog-kernel
lane_seq: 3
collision: [v7-datalog-check, v7-test]
blocked_by: ['@dl7-checked-foundation', '@dl7-datalog-rulings']
closed: 2026-08-29
closed_by: codex
commits:
- hash: cf974641f
  summary: Expose checked adjacent edge indices
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

- [x] The selected ordering contract has one checked representation.
- [x] Dense strict-before order can be derived using ordinary positive rules.
- [x] Empty and singleton edge lists produce deterministic rows.
- [x] Functional keys and row ordering have exact receipts.
- [x] No Pick, Exclude, SQL, or emitter implementation lands in this task.

## Tests Run

- [x] Existing consolidated V7 SWI suite with one exact ordering receipt.

## Resolution

### 2026-08-29T22:24:40Z · @codex

Checked predecessor rows, exact owner-index keys, deterministic empty and singleton behavior, and ordinary recursive strict-order derivation are covered. Focused SWI suite: 12/12. Tree-sitter: 1/1.
