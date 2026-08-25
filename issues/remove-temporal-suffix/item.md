---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: flash4
status: open
priority: normal
epic: userland-type-graph
labels:
- area:dl6
- area:parser
- intent:cleanup
- size:small
- model:small
size: S
lane: parser-surface
lane_seq: 30
collision: [parser-paths, conformance-fixtures]
blocked_by: ['@userland-temporal-annotations']
---

# Remove legacy space-delimited temporal syntax

## Description

Remove space-delimited `log keep(...)` declaration suffixes after call-form parity.

## Migration

```dl6
rel Event(id: int).
temporal.log(Event).
temporal.keep(Event, count(2)).
```

## Acceptance Criteria

- [ ] Parser, printer, CST, and diagnostics remove suffix modifiers.
- [ ] Repository fixtures and programs use call-form annotations.
- [ ] A named removed-surface diagnostic gives the replacement.
- [ ] Runtime retention and SQL plans remain unchanged.
- [ ] Documentation contains one temporal spelling.

## Tests Run

Parse/print, diagnostics, fixture compile pass, runtime parity.

## Implementation Notes

Execution tier: Small, size `S`, label `size:small`. Flash4 maximum-thinking Boop OpenCode lane with completion hail. Blocked by `@userland-temporal-annotations`.
