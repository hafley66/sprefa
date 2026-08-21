---
created: 2026-08-21
updated: 2026-08-21
type: task
assignee: chris
status: open
priority: normal
epic: comptime-type-model
labels:
- area:dl6
- intent:decision
related: ['@wrapper-composition', '@option-key-normalization', '@semantic-type-identity']
---

# Review identity for all-optional relations

## Description

Review storage identity choices for relations whose fields all lower through option storage. Produce a decision only.

Review relations that retain zero stored columns after option lowering. Compare
hidden identity rows, an explicit-key requirement, and direct owner storage.

## Review Alternatives

- Allocate a hidden identity row for each owner instance.
- Require an authored key whenever option lowering removes all visible columns.
- Store identity directly on the owning relation or reject zero-column
  relations.

## Acceptance Criteria

- [ ] Write the type signatures and instance lifetime for each candidate.
- [ ] Describe SQLite reads, writes, deletion, replacement, and uniqueness for each candidate.
- [ ] Decide whether an all-optional relation may exist without an authored key.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not change `reference_target_has_no_columns` from this card.
