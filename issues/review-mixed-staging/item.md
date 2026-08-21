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
related: ['@comptime-stage-elision', '@compiler-type-relations', '@type-annotation-eval']
---

# Review mixed compile-time and runtime arguments

## Description

Review phase inference and storage erasure for ordered compile-time and runtime arguments. Produce a decision only.

Review the lifetime of each argument in a mixed declaration such as
`rel Box(T: type, value: T)`: compile-time binding, specialization input,
runtime row, emitted artifact, and SQLite storage.

## Review Alternatives

- Stage every `: type` argument and erase it after specialization.
- Stage only invocations whose type arguments are compile-time ground and
  reject cross-stage uses.
- Require explicit curry groups for staging and leave mixed ordered arguments
  deferred.

`@comptime-stage-elision` is the pre-existing follow-up card. This card owns
the ruling that gates it; it does not authorize its implementation.

## Acceptance Criteria

- [ ] Specify the type signature and phase of every argument position.
- [ ] Specify when compile-time arguments are read and erased.
- [ ] Specify runtime storage for each concrete application.
- [ ] Decide whether later value arguments may infer earlier type arguments.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not remove `compiler_relation_mixed_domain` from this card.
