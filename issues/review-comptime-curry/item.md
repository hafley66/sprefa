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
related: ['@comptime-stage-elision', '@compiler-type-relations']
---

# Review explicit and automatic compile-time currying

## Description

Review coexistence of the existing generic curry group and inferred compile-time argument currying. Produce a decision only.

Compare these surfaces and their normalization:

```dl6
rel Box(T)(value: T).
rel Box(T: type, value: T).
```

Review explicit curry groups, automatic currying of a compile-time prefix, and
compile-time arguments that appear after a runtime argument.

## Review Alternatives

- Keep both authored forms and require one normalized semantic application.
- Keep explicit generic groups as the only staging boundary and defer inferred
  currying.
- Infer only a leading compile-time prefix; require an explicit group for any
  compile-time argument after a runtime argument.

Treat `@comptime-stage-elision` as follow-up work. This card records the
language ruling and does not authorize changes to that card's lowering.

## Acceptance Criteria

- [ ] Decide whether both surfaces remain legal.
- [ ] Decide where automatic currying stops.
- [ ] Decide whether explicit curry groups may lift later compile-time arguments across runtime arguments.
- [ ] Require or reject canonical-row equality between explicit and inferred forms.
- [ ] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not change parser or generic expansion from this card.
