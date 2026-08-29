---
created: 2026-08-29
updated: 2026-08-29
type: task
status: needs-info
priority: high
epic: dl7-engine-adapter
labels: [dl7, needs-ruling]
size: S
lane: dl7-program-json-rulings
lane_seq: 0
collision: [v7-engine-contract]
blocked_by: ['@dl7-layout-rulings']
---

# Resolve ProgramJson target adapter policies

## Description

Freeze target-specific naming, boundary type, DDL, statement, and seed-placement mappings for the ProgramJson adapter.

## Type signatures

```prolog
program_json_column_type(+LayoutRepresentation, -RowColumnType).
program_json_relation_name(+LayoutRelation, -Name).
program_json_relation_ddl(+LayoutRelation, -DdlStatements).
program_json_relation_statements(+LayoutRelation, -Add, -Delete, -Boundary).
program_json_seed_placement(+LayoutRelation, +Seeds, -BootOrArrivalPlan).
```

## Instance timelines

The target-neutral layout exists first. This adapter policy derives one
immutable ProgramJson artifact. Runtime tables remain owned by the existing
Rust engine.

## Storage, reads, writes, uniqueness

Each layout relation and artifact role maps to one ProgramJson relation plan.
The mapping owns quoted or encoded target names, `RowColumnType`, durable and
transient DDL, arrival add/delete and boundary statements, and boot versus
arrival seed placement.

## Required rulings

- [ ] Map each target-neutral layout representation to `RowColumnType`.
- [ ] Select quoted semantic spelling, delimiter encoding, or generated target
  names for relations and companion tables.
- [ ] Select durable and transient DDL ownership.
- [ ] Select add, delete, and boundary statement contracts.
- [ ] Select boot, arrival, or split seed placement.

## Acceptance Criteria

- [ ] Every required `IncrementalRelationPlan` field has one derivation.
- [ ] Semantic relation identity remains separate from target table spelling.
- [ ] No SQLite or ProgramJson vocabulary enters the type, rule, flow, or
  target-neutral layout graphs.
- [ ] Existing V6 ProgramJson behavior is named as parity or a deliberate
  changed contract field by field.

## Tests Run

- [ ] `issuectl doctor` reports no new errors or dependency cycles.
