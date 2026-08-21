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
related: ['@semantic-type-identity', '@type-relation-ir']
---

# Review higher-kinded type constructors

## Description

Review whether and when constructor-valued type arguments enter DL6. Produce a decision only.

Review constructor values such as `list`, `option`, and `Result`, including the
kind signatures needed for `Constructor(Element)`.

## Review Alternatives

- Defer constructor-valued parameters from this arc.
- Admit only closed, named constructors with fixed arity.
- Add first-class kind values such as `type -> type`, with explicit arity and
  partial-application checks.

## Acceptance Criteria

- [ ] Decide whether higher-kinded constructors belong in this arc or remain deferred.
- [ ] If included, choose the minimum kind representation.
- [ ] Decide how constructor arity and partial application are checked.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not add constructor-variable grammar from this card.
