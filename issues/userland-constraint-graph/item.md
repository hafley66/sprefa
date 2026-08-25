---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: terra
status: open
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:schema
- size:med
- model:medium
size: M
lane: storage-schema
lane_seq: 20
collision: [generic-type-core, storage-lowering]
blocked_by: ['@typegraph-member-planes', '@typed-annotation-corrections']
---

# Represent keys and constraints as user-land type graph facts

## Description

Represent primary keys, unique constraints, indexes, and foreign keys as first-class compiler rows derived by user-land DL6 annotations.

## Signatures

```dl6
$type.constraint(ConstraintId, OwnerId, Kind, Group).
$type.constraint_member(ConstraintId, Ordinal, MemberId).
```

One member is a scalar key. Multiple members form a composite constraint. Different IDs form alternate constraints. One primary key is allowed per relation; multiple unique and index constraints are allowed.

## Acceptance Criteria

- [ ] Existing `key(T)` columns derive one default composite primary key.
- [ ] Named groups represent independent constraints.
- [ ] Constraint identity is stable under unrelated declarations.
- [ ] Member order follows relation order unless explicitly authored.
- [ ] Empty, duplicate, conflicting, and cross-owner groups have diagnostics.
- [ ] No key-specific PL evaluator relation is added.

## Tests Run

Key wrapper, option key, composite, alternate, and canonical graph tests.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by member planes and `@typed-annotation-corrections`.
