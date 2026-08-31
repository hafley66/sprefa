---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: done
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-terra]
lane: dl7-datalog-kernel
lane_seq: 4
collision: [v7-datalog-check, v7-libtime, v7-test, v7-datalog-lower]
size: L
blocked_by: ['@dl7-compiler-split', '@dl7-checked-foundation', '@dl7-relational-cons']
closed: 2026-08-29
closed_by: codex
commits:
- hash: e4517c3eb
  summary: Lower prefix negation into checked polarity
- hash: e642d8c9f
  summary: Evaluate checked rules by dependency stratum
---

# Add checked stratified negation to DL7

## Description

Lower one prefix negative goal into explicit checked polarity, compute strict dependency gaps, and evaluate each stratum after its lower rows are complete.

## Signatures

checked_goal(+Polarity, +Call).
depends(+HeadRef, +BodyRef, +Polarity).
stratum(+RelationRef, +NonnegativeInteger).
evaluate(+Rules, +Seeds, -Closure, -Diagnostics).

## Instance lifetimes

Polarity and strata live in the checked program and semantic plan. Evaluation-local lower rows and SWI tables live for one evaluate/4 call and are cleaned on exit.

## Storage, reads, writes, uniqueness

Positive goals read the current stratum closure. Negative goals read only the completed lower-row set. A negative cycle is rejected. One relation receives one stratum number.

## Acceptance Criteria

- [x] `(not (Relation Argument...))` lowers without adding infix syntax.
- [x] Every checked body row has the exact form
  `checked_goal(positive|negative, call(ref(Relation), Arguments))`.
- [x] Negative variables are bound by preceding authored-order goals.
- [x] Negative dependencies impose `HeadStratum >= BodyStratum + 1`.
- [x] Negative cycles produce one deterministic diagnostic with sorted relation
  payload and source origin.
- [x] Positive recursive closure remains term-identical.
- [x] Negative goals read completed lower rows only.
- [x] Evaluation-local lower rows, asserted clauses, and SWI tables are absent
  after success, diagnostic, and exception.
- [x] Negative constructive-kernel goals follow the selected ruling in
  `@dl7-datalog-rulings`.
- [x] No emitter or target storage code changes.

## Tests Run

- [x] Existing consolidated V7 SWI suite with positive, anti-join, and negative-cycle receipts in the existing test file.

## Implementation Notes

Donor predicates: compiler_rule_constraint/5, relax_compiler_strata/4,
tabled_compiler_closure/4, satisfy_tabled_compiler_body/2. Shared checked-goal,
safety, key, and strata contracts land in `@dl7-checked-foundation` first.

## Resolution

### 2026-08-29T22:33:05Z · @codex

Prefix negative lowering, produced-variable safety, strict dependency gaps, source-positioned cycle diagnostics, completed-lower evaluation, negative kernel refusal, positive parity, and cleanup receipts are covered. Focused SWI suite: 13/13. Tree-sitter: 1/1.
