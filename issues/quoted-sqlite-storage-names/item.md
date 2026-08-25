---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: terra
status: open
priority: normal
epic: relational-semantic-planes
labels:
- area:sqlite
- area:compiler
- intent:storage
- size:med
- model:medium
size: M
lane: materialization
lane_seq: 20
collision: [storage-lowering, type-emitters]
blocked_by: ['@userland-materialization-graph']
---

# Preserve semantic paths in quoted SQLite identifiers

## Description

Map logical materialization IDs to SQLite identifiers in the SQLite target plan. Semantic identity remains independent of physical spelling.

## Target-Plan Shape

```text
storage relation ID + structural artifact role
  -> SQLite physical name
  -> quoted SQLite identifier
```

## Rendering Rules

- Quote every identifier.
- Escape embedded quotes by doubling them.
- Preserve dots, hyphens, spaces, and approved Unicode.
- Reject NUL and values the SQLite API cannot represent.
- Diagnose ASCII case-fold collisions.
- Derive delta, frontier, index, trigger, dictionary, and refcount companion names from structural artifact roles.

## Acceptance Criteria

- [ ] Target-plan rows select physical names without changing semantic IDs.
- [ ] Identifier escaping handles embedded quotes.
- [ ] Dotted and hyphenated names execute in SQLite.
- [ ] Case-only and companion-artifact collisions are deterministic or diagnosed.
- [ ] Other target plans can select different spellings from the same materialization rows.
- [ ] Existing names remain byte-identical without an authored mapping.

## Tests Run

Lowerer snapshots, executable SQLite DDL, module storage, embedded-quote and collision fixtures.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification. Authored source identifier syntax remains outside this card.
