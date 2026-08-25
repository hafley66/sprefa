---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: flash4
status: open
priority: normal
epic: userland-type-graph
labels:
- area:sqlite
- intent:schema
- size:small
- model:small
size: S
lane: storage-schema
lane_seq: 30
collision: [storage-lowering]
blocked_by: ['@userland-constraint-graph']
---

# Emit SQLite constraints from type graph facts

## Description

Render normalized constraint rows as SQLite table and index DDL. Grouping and SQL rendering are mechanical; constraint meaning remains in DL6.

## Output Examples

```sql
PRIMARY KEY ("tenant_id", "user_id")
UNIQUE ("email")
CREATE INDEX "name" ON "table" (...)
```

## Acceptance Criteria

- [ ] Primary, unique, index, and supported foreign-key rows render deterministically.
- [ ] Composite order follows ordinal rows.
- [ ] Existing key SQL, conflict targets, replacement, and stale retraction remain equal.
- [ ] Emission contains no annotation interpretation.
- [ ] Focused DDL executes against SQLite.

## Tests Run

DDL snapshots, SQLite execution, key timeline, lowerer regression tests.

## Implementation Notes

Execution tier: Small, size `S`, label `size:small`. Flash4 maximum thinking through a Boop OpenCode lane. Completion hail and independent artifact verification are required.
