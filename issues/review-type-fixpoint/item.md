---
created: 2026-08-21
updated: 2026-08-21
type: task
assignee: chris
status: open
priority: high
epic: comptime-type-model
labels:
- area:dl6
- intent:decision
related: ['@compiler-type-relations', '@semantic-type-identity']
---

# Review type evaluation and specialization fixpoint

## Description

Review how type-returning relations and generic discovery iterate to a stable result. Produce a decision only.

Review evaluation order for `Box(first(int))`, compiler facts carrying
`Box(int)`, nested applications, recursion, stable identities, and termination.

## Review Alternatives

- Retain one-pass evaluation and reject type applications discovered later.
- Use a bounded evaluate/discover/specialize/freeze fixpoint.
- Permit generated types to trigger another query only within an explicit
  query-depth or frontier limit.

## Acceptance Criteria

- [ ] Compare one-pass staging with a bounded evaluate/discover/specialize/freeze fixpoint.
- [ ] Specify the frontier identity and termination condition.
- [ ] Specify duplicate, oscillation, and limit diagnostics.
- [ ] Decide whether generated types may trigger another type query.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not reorder expansion phases from this card.
