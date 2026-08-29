---
created: 2026-08-29
updated: 2026-08-29
type: task
status: needs-info
priority: high
epic: dl7-engine-adapter
labels: [dl7, needs-ruling]
size: S
lane: dl7-layout
lane_seq: 0
collision: [v7-layout-contract]
---

# Resolve DL7 layout policy rows

## Description

Freeze storage selection, physical type, key, naming, durability, statement, and seed-placement policies for the layout planner.

## Type signatures

```prolog
relation_storage_kind(+Relation, -Kind).
relation_key_indices(+Relation, -KeyIndices).
physical_column_type(+SemanticType, -BoundaryType).
physical_relation_name(+Relation, +Target, -Name).
physical_table_ddl(+RelationLayout, -DdlStatements).
physical_relation_statements(+RelationLayout, -Add, -Delete, -Boundary).
seed_placement(+Relation, +Seeds, -BootOrArrivalPlan).
```

## Instance timelines

Logical checked rows exist first. Target-neutral layout rows exist next.
Target adapters consume layout rows and emit physical DDL and statements.
Runtime tables remain engine-owned.

## Storage, reads, writes, uniqueness

One stored logical relation must map to one physical relation identity. This
task settles stored-relation selection, `set|log`, key positions, semantic to
boundary type mapping, quoted or encoded names, durable and transient table
ownership, add/delete/boundary statements, and authored seed placement.

## Required rulings

- [ ] Identify which checked relations are stored and how `set|log` is derived.
- [ ] Select explicit key positions for the first stored relation slice.
- [ ] Map V7 semantic types, including `any` and `type`, to boundary types.
- [ ] Select the target-neutral relation name and target physical-name rule.
- [ ] Select durable and transient DDL ownership.
- [ ] Select add, delete, and boundary statement contracts.
- [ ] Select boot, arrival, or split seed placement.

## Acceptance Criteria

- [ ] Each missing planner input row has one selected contract.
- [ ] Target-neutral layout data contains no SQL dialect spelling.
- [ ] The ProgramJson writer receives every required engine field through a
  target adapter.

## Tests Run

- [ ] `issuectl doctor` reports no new errors or dependency cycles.

## Implementation Notes

Evidence and existing engine fields: `v7/tasks/results/10_LAYOUT_BLOCKER.md`.
