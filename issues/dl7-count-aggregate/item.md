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
lane_seq: 2
collision: [v7-datalog-lower, v7-datalog-check, v7-libtime, v7-test]
size: L
blocked_by: ['@dl7-stratified-negation']
---

# Add completed-stratum count aggregate to DL7

## Description

Lower a nested count application in a rule head into checked aggregate data and derive grouped counts from completed lower rows. This supplies dense predecessor counts for selected ordered edges.

## Signatures

aggregate_argument(count, +Expression).
derive_aggregate(+CompletedRows, +Rule, -Row).

## Instance lifetimes

Aggregate syntax nodes live through lowering. Checked aggregate descriptors live in the checked program. Group bags live only during one completed-stratum fold. Derived count rows join the normal closure afterward.

## Storage, reads, writes, uniqueness

Plain head arguments form the group key. count(Expression) contributes one row per successful body binding. Each group emits one result row. Aggregate dependencies impose stratum gap 1.

## Acceptance Criteria

- [ ] Nested head application lowers through the generic expression path.
- [ ] Only count is admitted in this card.
- [ ] Count reads a completed lower stratum.
- [ ] Group keys and output rows are deterministic and sorted.
- [ ] Aggregate recursion is rejected.
- [ ] No SQL or emitter vocabulary enters V7 Prolog.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with grouped and dense-rank receipts in the existing test file.

## Implementation Notes

Donor predicates: compiler_aggregate_head/2, derive_compiler_aggregate_row/3, compiler_aggregate_group_key/3, compiler_aggregate_arguments/3.
