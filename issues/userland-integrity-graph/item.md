---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: codex
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:schema
- size:med
- model:medium
size: M
lane: integrity-schema
lane_seq: 10
collision: [generic-type-core, storage-lowering]
blocked_by: ['@semantic-plane-rulings', '@relation-application-semantics', '@member-edge-relational-view']
---

# Derive integrity facts in user-land DL6

## Description

Represent logical row identity and relational integrity as ordinary DL6 facts. Target emitters consume these facts and decide which capabilities they can realize.

## Signatures

```dl6
rel integrity.key(Owner: key(type), Group: key(type), Member: type.member, Index: int).
rel integrity.unique(Owner: key(type), Group: key(type), Member: type.member, Index: int).
rel integrity.reference(Owner: key(type), Group: key(type), Member: type.member,
                        TargetOwner: type, TargetMember: type.member, Index: int).
```

`key(Target)` remains an ordinary relation application in the type graph. User-land rules convert its annotation evidence into integrity rows.

## Lifetime

Integrity rows are recomputed during compiler refreeze. They exist independently of any target plan.

## Storage And Uniqueness

Constraint groups are keyed by `(Owner, Kind, Group)`. Member order uses `Index`. One primary key group is allowed per owner; unique and reference groups may repeat under distinct group IDs.

## Acceptance Criteria

- [ ] Existing `key(T)` members derive one composite primary-key group.
- [ ] Multiple keyed members remain one relation identity.
- [ ] Named groups represent independent unique and reference constraints.
- [ ] Empty, duplicate, conflicting, and cross-owner groups have diagnostics.
- [ ] Integrity rows contain no SQLite or emitter vocabulary.
- [ ] Index/performance hints remain outside the Integrity Graph.

## Tests Run

Key wrapper, option-key rejection, composite key, alternate unique, reference, diagnostics, compiler fixpoint tests.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification.
