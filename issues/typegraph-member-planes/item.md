---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: codex
status: done
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 30
collision: [generic-type-core, storage-lowering]
blocked_by: ['@typegraph-node-edge-view', '@canonical-storage-projection']
commits:
- hash: f00247804
  summary: expose logical member/project sources and derive SQLite storage planes
- hash: 07403de4a
  summary: retain canonical logical types beside physical storage in relplans
closed: 2026-08-24
closed_by: codex
---

# Separate authored and storage member planes

## Description

Distinguish logical/authored members from physical/storage members. Current lowering can retain the same owner, position, and name with different targets, such as `option(text)` and its stored integer representation.

## Provisional Signature

```dl6
type.member(MemberId, OwnerId, Plane, Position, Name, TargetTypeId).
```

If specialized rows remain canonical, expose an equivalent plane relation instead of duplicating IDs.

## Timeline

Logical rows originate from authored or generated schemas. Storage rows originate after option, enum, relation-value, and target rewriting. Type operators read logical rows; emitters read storage rows.

## Acceptance Criteria

- [x] Logical and storage rows are distinguishable without name heuristics.
- [x] Option, enum, relation-value, anonymous, and generic cases have receipts.
- [x] Projections default to logical rows.
- [x] Emitters consume storage rows without copied logical fields.
- [x] Member identities remain stable or have a documented migration.

## Tests Run

- Focused member and storage graph: 18/18 passed.
- Emitter and catalog consumer matrix: 148/148 passed.
- Full `v6/prolog/compile/test/plunit_tests.pl`: 1094/1094 passed in 16
  seconds.
- Receipts cover option, enum, relation-value, anonymous product, generic
  application, list, relation ID, module, TypeScript/Rust emission, and SQLite
  execution paths.

## Implementation Notes

- `type.member/6` is a compiler-fixpoint source over the `logical` plane.
- `type.project/3` reads logical targets by default.
- `member_plane_rows/3` derives `storage(sqlite)` rows after relplan completion
  by joining `storage_column/2` through `MemberId`.
- Final compatibility relplans retain `declared(LogicalType)` and use the
  storage row for their physical third field.
- `MemberId` remains `member(OwnerTypeId, Position, Name)` in both planes.
- Legacy `type_member/5` remains available during migration.

## Resolution

### 2026-08-25T01:59:06Z · @codex

Added dotted type.member/6 and type.project/3 logical compiler sources; derived storage(sqlite) rows by joining storage_column through the same MemberId; rebuilt final relplans from canonical storage rows while retaining logical declared types. Compiler-fixpoint sources expose logical rows because physical storage exists after relplan completion. Verification: focused member/storage graph 18/18, emitter and catalog matrix 148/148, full Prolog suite 1094/1094 in 16 seconds.
