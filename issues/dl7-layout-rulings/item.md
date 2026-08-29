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

Freeze the target-neutral storage selection, artifact-role, key, and encoded
column representation policies required by the layout planner.

## Type signatures

```prolog
relation_storage_kind(+Relation, -Kind).
relation_key_indices(+Relation, -KeyIndices).
layout_column_representation(+SemanticType, -Representation).
layout_artifact(+Relation, -ArtifactRole).
```

## Instance timelines

Logical checked rows exist first. Target-neutral layout rows exist next. Target
adapters consume immutable layout rows. Runtime artifacts remain engine-owned.

## Storage, reads, writes, uniqueness

One stored logical relation may produce one or more artifact roles. This task
settles stored-relation selection, `set|log`, key positions, artifact roles,
and target-neutral encoded column representations. SQL names, DDL, statements,
and ProgramJson seed placement belong to `@dl7-program-json-rulings`.

## Required rulings

- [ ] Identify which checked relations are stored and how `set|log` is derived.
- [ ] Select explicit key positions for the first stored relation slice.
- [ ] Select artifact roles for the first stored relation slice.
- [ ] Define target-neutral encoded representations for V7 semantic types,
  including `any` and `type`.

## Acceptance Criteria

- [ ] Each missing planner input row has one selected contract.
- [ ] Target-neutral layout data contains no SQL dialect spelling, table name,
  DDL, or engine statement.
- [ ] Target adapters can derive their physical fields without changing
  semantic relation identity.

## Tests Run

- [ ] `issuectl doctor` reports no new errors or dependency cycles.

## Implementation Notes

Evidence and existing engine fields: `v7/tasks/results/10_LAYOUT_BLOCKER.md`.
