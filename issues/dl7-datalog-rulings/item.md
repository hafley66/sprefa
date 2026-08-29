---
created: 2026-08-29
updated: 2026-08-29
type: task
status: needs-info
priority: high
epic: dl7-datalog-extensions
labels: [dl7, needs-ruling]
size: S
lane: dl7-datalog-rulings
lane_seq: 0
collision: [v7-design]
---

# Resolve DL7 Datalog semantic rulings

## Description

Freeze the semantic choices required before checked Datalog foundation work.
The implementation review is `v7/design/3_DATALOG_EXTENSIONS.REVIEW.md`.

## Type signatures

```prolog
cons(?Head, ?Tail, ?List).
relation(ref(RelationIdentity), +Arity, +KeySets).
predecessor(+Owner, +EarlierIndex, +LaterIndex).
```

## Instance timelines

The selected contracts enter checked Datalog and remain fixed through one
evaluation. They do not select an emitter or storage backend.

## Storage, reads, writes, uniqueness

This task writes policy only. It must settle whether derived rows are checked
against relation functional keys and what ordering rows are visible to
userland rules.

## Required rulings

- [ ] Empty cons: `const([])` has no `cons/3` tuple and
  `const(symbol(nil))` remains the empty-tail sentinel, or an exact replacement
  row is specified.
- [ ] Ordered indices: choose explicit `predecessor/3` rows, a checked ground
  integer comparison, or comparison plus subtraction.
- [ ] Zero counts: retain the V6 empty-bag behavior plus a negative zero-rank
  rule, or define an anchored aggregate that emits zero.
- [ ] Functional keys: authorize relation key metadata and final closure
  validation, including both settled keys of `':'/4`.
- [ ] Negative kernel goals: reject negative `cons/3` and `intern/3`, or define
  completed-row lookup semantics for them.

## Acceptance Criteria

- [ ] Every required ruling has one selected term-level contract.
- [ ] The checked-foundation, ordered-index, cons, negation, and count cards
  carry the selected contracts without contradictory acceptance criteria.
- [ ] No emitter vocabulary enters the checked Datalog contracts.

## Tests Run

- [ ] `issuectl doctor` reports no new errors or dependency cycles.
