---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: terra
status: open
priority: normal
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:projection
- size:med
- model:medium
size: M
lane: dot-path
lane_seq: 20
collision: [generic-type-core, parser-paths]
blocked_by: ['@typegraph-member-planes', '@dot-brace-nesting']
---

# Derive dot projection through user-land type rules

## Description

Move member and nested-relation projection from PL experiment code into user-land DL6 over canonical graph rows.

## Signature

```dl6
$type.project(OwnerId, Name, TargetId).
```

Logical members and nested edges derive projection rows. The dependency is `(Owner, Name) -> Target`. Equal targets deduplicate; distinct targets produce the named ambiguity diagnostic.

## Acceptance Criteria

- [ ] Projection logic is authored DL6 rather than `5a_type_projection.pl`.
- [ ] Members, nested relations, and approved variants share one lookup relation.
- [ ] Logical/storage dual rows create no false conflicts.
- [ ] Deep dots resolve in every reference-bearing position.
- [ ] Existing conflict, deduplication, inline-sum, and diagnostic receipts remain exact.

## Tests Run

Braced nesting, canonical reflection, and complete compiler tests.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by member planes and `@dot-brace-nesting`.
