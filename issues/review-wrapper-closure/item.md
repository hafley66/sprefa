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
related: ['@wrapper-composition', '@key-wrapper-normalization', '@option-key-normalization', '@type-relation-ir']
---

# Review wrapper and anonymous type closure

## Description

Review recursive wrapper normalization and anonymous type participation after substitution. Produce a decision only.

Review closure for `list(option(T))`, `option(list(T))`, relation elements,
anonymous products, anonymous sums, generic substitution, and generated enum
storage.

## Review Alternatives

- Normalize named and anonymous types through one substitution and wrapper
  closure path.
- Support named wrappers first and defer anonymous wrapper closure.
- Preserve each current refusal as a bounded semantic boundary until a later
  review.

`@wrapper-composition` remains the follow-up lowering card. This card owns the
review classification and does not loosen its refusals.

## Acceptance Criteria

- [ ] Enumerate the supported recursive wrapper matrix.
- [ ] Decide whether anonymous and named types use the identical normalization path.
- [ ] Specify normalization timing relative to substitution and option/enum lowering.
- [ ] Classify each current refusal as semantic, deferred, or a defect.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not loosen wrapper refusals from this card.
