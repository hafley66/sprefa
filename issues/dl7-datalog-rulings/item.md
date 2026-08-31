---
created: 2026-08-29
updated: 2026-08-29
type: task
status: done
priority: high
epic: dl7-datalog-extensions
labels: [dl7, needs-ruling]
size: S
lane: dl7-datalog-rulings
lane_seq: 0
collision: [v7-design]
closed: 2026-08-29
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

- [x] Empty cons: `const([])` has no `cons/3` tuple and
  `const(symbol(nil))` remains the empty-tail sentinel, or an exact replacement
  row is specified.
- [x] Ordered indices: choose explicit `predecessor/3` rows, a checked ground
  integer comparison, or comparison plus subtraction.
- [x] Zero counts: retain the V6 empty-bag behavior plus a negative zero-rank
  rule, or define an anchored aggregate that emits zero.
- [x] Functional keys: authorize relation key metadata and final closure
  validation, including both settled keys of `':'/4`.
- [x] Negative kernel goals: reject negative `cons/3` and `intern/3`, or define
  completed-row lookup semantics for them.

## Acceptance Criteria

- [x] Every required ruling has one selected term-level contract.
- [x] The checked-foundation, ordered-index, cons, negation, and count cards
  carry the selected contracts without contradictory acceptance criteria.
- [x] No emitter vocabulary enters the checked Datalog contracts.

## Tests Run

- [x] `issuectl doctor` reports no new errors or dependency cycles.

## Decisions

### 2026-08-29T21:59:57Z · @codex

Bounded first-slice contracts: (1) const([]) has no cons/3 tuple and const(symbol(nil)) remains the empty-tail sentinel; (2) checked adjacent predecessor(Owner, EarlierIndex, LaterIndex) rows supply ordered indices; (3) an empty aggregate bag emits no group and userland rank rules supply the zero arm through stratified negation; (4) checked relation/3 rows carry key sets and final closure validates every declared functional dependency, including ':'/4 keys [0,1] and [0,3]; (5) checked negative cons/3 and intern/3 goals are rejected in the first negation slice.
