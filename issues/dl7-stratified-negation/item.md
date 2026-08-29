---
created: 2026-08-29
updated: 2026-08-29
type: task
assignee: terra
status: open
priority: high
epic: dl7-datalog-extensions
labels: [dl7, model-terra]
lane: dl7-datalog-kernel
lane_seq: 1
collision: [v7-datalog-check, v7-libtime, v7-test]
size: L
blocked_by: ['@dl7-compiler-split']
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

- [ ] Prefix negative goal lowers without adding infix syntax.
- [ ] Checked rule bodies carry explicit positive or negative polarity.
- [ ] Negative dependencies impose gap 1.
- [ ] Negative cycles produce one deterministic diagnostic.
- [ ] Positive recursive closure remains unchanged.
- [ ] No emitter or target storage code changes.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with positive, anti-join, and negative-cycle receipts in the existing test file.

## Implementation Notes

Donor predicates: compiler_rule_constraint/5, relax_compiler_strata/4, tabled_compiler_closure/4, satisfy_tabled_compiler_body/2.
