---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: terra
status: open
priority: normal
epic: userland-type-graph
labels:
- area:sqlite
- area:compiler
- intent:storage
- size:med
- model:medium
size: M
lane: storage-schema
lane_seq: 10
collision: [storage-lowering, type-emitters]
blocked_by: ['@typegraph-integration-plan', '@canonical-storage-projection']
---

# Preserve semantic paths in quoted SQLite identifiers

## Description

Allow SQLite physical tables and companion objects to preserve approved punctuation through quoted identifiers. Semantic relation identity and source path remain separate from backend spelling.

## Signature

```dl6
$storage.name(TargetId, sqlite, PhysicalName).
```

## Rendering Rules

- Quote every identifier.
- Escape embedded quotes by doubling them.
- Preserve dots, hyphens, spaces, and approved Unicode.
- Reject NUL and values the SQLite API cannot represent.
- Retain SQLite ASCII case-fold collision handling.
- Reserve or structurally derive delta, frontier, index, trigger, dictionary, and refcount companion names.

## Acceptance Criteria

- [ ] Comptime rows select physical names without changing semantic IDs.
- [ ] Identifier escaping handles embedded quotes.
- [ ] Dotted and hyphenated names execute in SQLite.
- [ ] Case-only and helper collisions are deterministic or diagnosed.
- [ ] TS and Rust plans carry the same spelling.
- [ ] Existing names remain byte-identical without opt-in mapping.

## Tests Run

Lowerer tests, executable SQLite DDL, module storage, TS/Rust snapshots.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Authored source identifiers containing punctuation are outside this card.
