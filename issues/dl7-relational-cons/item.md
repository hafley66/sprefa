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
lane_seq: 0
collision: [v7-libtime, v7-test]
size: S
blocked_by: ['@dl7-compiler-split']
---

# Make DL7 cons relational in bounded modes

## Description

Extend the existing kernel cons relation so a ground list can bind its head and tail while the existing ground head plus tail mode still constructs the list.

## Signatures

cons(?Head, ?Tail, ?List).

Allowed modes: ground Head and Tail determine List; ground nonempty List determines Head and Tail. Reject a call with neither side ground.

## Instance lifetimes

Arguments live inside one evaluator proof. Constructed or deconstructed list constants remain ordinary closure terms. No global list store is introduced.

## Storage, reads, writes, uniqueness

List values remain const([...]). Construction is functional on (Head, Tail). Destructuring is functional on List. Closure rows remain set-valued.

## Acceptance Criteria

- [ ] Existing construction behavior remains byte-equivalent.
- [ ] Ground nonempty lists deconstruct deterministically.
- [ ] Empty-list behavior is explicit and deterministic.
- [ ] Underconstrained calls produce a named diagnostic or checked-program refusal.
- [ ] No Pick or Exclude name appears in Prolog.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with one added oracle arm and no new test file.
