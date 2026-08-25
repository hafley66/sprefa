---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: flash4
status: open
priority: normal
epic: relational-semantic-planes
labels:
- area:sqlite
- intent:schema
- size:small
- model:small
size: S
lane: integrity-schema
lane_seq: 20
collision: [storage-lowering]
blocked_by: ['@userland-integrity-graph']
---

# Emit SQLite integrity from target-neutral facts

## Description

Render `integrity.*` rows into SQLite DDL. Grouping and SQL rendering are target work; annotation interpretation remains in DL6.

## Inputs

```text
integrity.key
integrity.unique
integrity.reference
storage relation and column mappings
SQLite target capability facts
```

## Outputs

```sql
PRIMARY KEY ("tenant_id", "user_id")
UNIQUE ("email")
FOREIGN KEY ("owner_id") REFERENCES "owner" ("id")
```

## Acceptance Criteria

- [ ] Primary, unique, and supported reference groups render deterministically.
- [ ] Composite member order follows integrity `Index`.
- [ ] Existing key SQL, conflict targets, replacement, and stale retraction remain equal.
- [ ] Emission contains no type-annotation interpretation.
- [ ] Unsupported integrity capabilities receive target diagnostics.
- [ ] Focused DDL executes against SQLite.

## Tests Run

DDL snapshots, executable SQLite DDL, key timeline, lowerer regression tests.

## Implementation Notes

Execution tier: Small, size `S`, model `small`. Flash4 maximum thinking through a Boop OpenCode lane; completion notification and artifact review required.
