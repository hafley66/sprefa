---
created: 2026-08-25
updated: 2026-08-25
type: feature
assignee: terra
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 50
collision: [generic-type-core]
blocked_by: ['@semantic-plane-rulings']
---

# Expose members as relational type edges

## Description

# Expose members as relational type edges

## Description

Expose the canonical member edge to user-land DL6 while preserving compatibility rows until the carrier-retirement ruling lands.

## Signature

```dl6
rel type.member(
  Owner: key(type),
  Name: key(text),
  Target: type,
  Index: int
).
```

## Lowering Sketch

```text
declared relation member
  -> canonical Owner and Target IDs
  -> one type.member edge
  -> compatibility projection for existing $type.member consumers
```

## Lifetime

The edge exists during compiler refreeze and is retracted when the owning declaration disappears. Unrelated declarations do not alter its semantic identity.

## Storage And Uniqueness

The logical key is `(Owner, Name)`. `Index` preserves authored order. Any temporary `MemberId` bridge is compiler storage metadata rather than the user-land edge identity.

## Acceptance Criteria

- [ ] DL6 can query and derive from the four-field member edge.
- [ ] Duplicate `(Owner, Name)` edges receive a diagnostic.
- [ ] `Index` is deterministic under unchanged authored member order.
- [ ] Existing consumers have an explicit compatibility projection or migration.
- [ ] No backend or physical storage fields appear in `type.member`.

## Tests Run

Focused compiler fixpoint tests, member identity tests, projection tests, full PLUnit compiler suite.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification. Blocked by the carrier and member-identity rulings.
