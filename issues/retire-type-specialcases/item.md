---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: terra
status: open
priority: normal
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:cleanup
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 60
collision: [generic-type-core, storage-lowering]
blocked_by: ['@userland-dot-projection', '@userland-constraint-graph', '@userland-temporal-annotations', '@userland-type-operators']
---

# Retire superseded host compiler type special cases

## Description

Remove feature-specific compiler semantics after their DL6 replacements and target rows are proven. Every removed predicate requires a reference-count receipt.

## Candidate Removals

- Temporal request builtin handling.
- PL projection grouping after user-land projection.
- Key wrapper collection superseded by constraint rows.
- Dead transport declarations, diagnostics, and tests.

## Acceptance Criteria

- [ ] Every candidate has before and after call-site counts.
- [ ] DL6 owns temporal, projection, constraints, and type operators.
- [ ] Host PL retains only approved generic mechanisms.
- [ ] Migrated fixtures keep runtime and generated artifact equality.
- [ ] No hidden compatibility shim restores removed semantics.

## Tests Run

Complete PLUnit, typegen golden, fixture matrix, TS/Rust execution, SQLite timelines.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Deletion starts after all replacement cards.
