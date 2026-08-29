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
lane_seq: 5
collision: [v7-datalog-lower, v7-datalog-check, v7-libtime, v7-test]
size: L
blocked_by: ['@dl7-stratified-negation']
---

# Add completed-stratum count aggregate to DL7

## Description

Lower one count form in a rule head into checked aggregate data and derive
grouped counts from completed lower rows. Dense selected-edge indices remain a
separate consumer of `@dl7-ordered-index` and the selected zero-count policy.

## Signatures

aggregate(count, +Expression).
derive_aggregate_rows(+CompletedRows, +AggregateRule, -SortedRows, -Diagnostics).

## Instance lifetimes

Aggregate syntax nodes live through lowering. Checked aggregate descriptors live in the checked program. Group bags live only during one completed-stratum fold. Derived count rows join the normal closure afterward.

## Storage, reads, writes, uniqueness

Plain head arguments form the group key. count(Expression) contributes one row per successful body binding. Each group emits one result row. Aggregate dependencies impose stratum gap 1.

## Acceptance Criteria

- [ ] One `(count Argument)` head form lowers into
  `aggregate(count, Argument)`; other nested head forms remain rejected.
- [ ] Exactly one count position is admitted in this card.
- [ ] Count reads a completed lower stratum.
- [ ] Every relation read by an aggregate-headed rule imposes stratum gap 1.
- [ ] Plain positions form the group key and one complete body proof contributes
  one bag entry, including equal values from distinct proofs.
- [ ] Empty-bag behavior matches `@dl7-datalog-rulings`.
- [ ] Group rows are deterministic and sorted.
- [ ] Aggregate recursion and malformed placement have distinct deterministic
  diagnostics.
- [ ] No SQL or emitter vocabulary enters V7 Prolog.

## Tests Run

- [ ] Existing consolidated V7 SWI suite with grouped count receipts in the
  existing test file.

## Implementation Notes

Donor predicates: compiler_aggregate_head/2, derive_compiler_aggregate_row/3, compiler_aggregate_group_key/3, compiler_aggregate_arguments/3.
