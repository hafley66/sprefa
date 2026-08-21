---
created: 2026-08-19
updated: 2026-08-21
type: task
status: open
priority: low
epic: applicative-type-annotations
labels:
- area:language
- deferred
blocked_by: ['@type-annotation-eval', '@review-mixed-staging']
related: ['@review-mixed-staging']
---

# Mixed-stage relations and comptime elision

## Description

Specify explicit mixed-stage relation parameters and comptime-elision rules
after applicative annotations establish the initial compile-time invocation
model. The earlier provisional rule required every argument to a `return:type`
invocation to be compile-time known. The final staging policy is decided on
`@review-mixed-staging`.

## Acceptance Criteria

- [ ] Mixed compile-time/runtime parameter signatures are defined.
- [ ] Type values remain ordinary typed relation values where stage permits.
- [ ] Elision, specialization identity, erasure, and cross-stage diagnostics are specified.

## Tests Run

## Implementation Notes

The language ruling for this follow-up is recorded on `@review-mixed-staging`.
This card remains open for the post-ruling specification and implementation
work; it does not settle the staging policy independently.
