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

# Expose primordial type edges

## Description

Expose the canonical named edge between type nodes to user-land DL6 while preserving compatibility rows until the carrier-retirement ruling lands.

## Signature

```dl6
rel type.edge(
  Owner: key(type),
  Name: key(text),
  Target: type,
  Index: int
).
```

## Lowering Sketch

```text
declared relation column
  -> canonical Owner and Target IDs
  -> one type.edge fact
  -> compatibility projection for existing $type.member consumers
```

## Lifetime

The edge exists during compiler refreeze and is retracted when the owning declaration disappears. Unrelated declarations do not alter its semantic identity.

## Storage And Uniqueness

The edge row itself is first-class and has logical key `(Owner, Name)`. `Index` preserves authored order. No synthetic `EdgeId` is introduced. Any temporary legacy `MemberId` bridge is compiler storage metadata rather than the user-land edge identity.

## Acceptance Criteria

- [ ] DL6 can query and derive from the four-field type edge.
- [ ] Duplicate `(Owner, Name)` edges receive a diagnostic.
- [ ] A DL6 relation can refer to the keyed edge row using the surface selected by the rulings card.
- [ ] `Index` is deterministic under unchanged authored edge order.
- [ ] Existing consumers have an explicit compatibility projection or migration.
- [ ] No backend or physical storage fields appear in `type.edge`.

## Tests Run

Focused compiler fixpoint tests, edge identity tests, projection tests, full PLUnit compiler suite.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification. Blocked by the carrier and edge-identity rulings.
