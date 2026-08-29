---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: luna
status: open
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-luna]
lane: dl7-datalog-kernel
lane_seq: 1
collision: [v7-libtime, v7-test, v7-datalog-check]
size: S
blocked_by: ['@dl7-compiler-split', '@dl7-checked-foundation']
---

# Make DL7 cons relational in bounded modes

## Description

Extend the existing kernel cons relation so a ground list can bind its head and tail while the existing ground head plus tail mode still constructs the list.

## Signatures

cons(?Head, ?Tail, ?List).

Allowed modes: ground Head and Tail determine List; ground nonempty List
determines Head and Tail. Checked authored-order validation rejects a call when
neither determining side is bound at that goal position.

## Instance lifetimes

Arguments live inside one evaluator proof. Constructed or deconstructed list constants remain ordinary closure terms. No global list store is introduced.

## Storage, reads, writes, uniqueness

List values remain const([...]). Construction is functional on (Head, Tail). Destructuring is functional on List. Closure rows remain set-valued.

## Acceptance Criteria

- [ ] Existing singleton and longer-list construction terms remain exactly
  equal under `==`.
- [ ] Ground nonempty lists deconstruct deterministically.
- [ ] Empty-list and improper-list behavior match the selected ruling in
  `@dl7-datalog-rulings`.
- [ ] Underconstrained source calls produce one named checker diagnostic with
  source origin before evaluator state is installed.
- [ ] Finite proper-list traversal reaches the nil tail without inventing a
  stored list identity.
- [ ] No Pick or Exclude name appears in Prolog.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with one added oracle arm and no new test file.

## Implementation Notes

This task follows `@dl7-checked-foundation` and the exact evaluator contract in
`v7/design/3_DATALOG_EXTENSIONS.REVIEW.md`.
