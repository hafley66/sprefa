---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: codex
status: in-progress
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
type.project(OwnerId, Name, TargetId).
```

Canonical member, variant, and nested edges derive projection rows. The dependency is `(Owner, Name) -> Target`. Equal targets deduplicate; distinct targets produce the named ambiguity diagnostic. Physical storage rows do not enter this relation.

## Required Lowering

```dl6
type.project(Owner, Name, Target) <-
  type.edge(_, Owner, 'member', _, Name, Target).

type.project(Owner, Name, Target) <-
  type.edge(_, Owner, 'variant', _, Name, Target).

type.project(Owner, Name, Target) <-
  type.edge(_, Owner, 'nested', _, Name, Target).
```

`type.project/3` remains a declared compiler relation with key `(Owner, Name)`. The authored DL6 rules fill it during the compiler fixpoint. Surface dotted-name parsing remains in the path resolver; this card replaces the semantic projection oracle used by compiler rules.

## Acceptance Criteria

- [ ] Projection logic is authored DL6 rather than `5a_type_projection.pl`.
- [ ] Members, nested relations, and approved variants share one lookup relation.
- [ ] Physical storage rows never enter the semantic lookup relation.
- [ ] Deep dots resolve in every reference-bearing position.
- [ ] Existing conflict, deduplication, inline-sum, and diagnostic receipts remain exact.

## Tests Run

Braced nesting, canonical reflection, and complete compiler tests.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Implemented directly in an isolated worktree. `@remove-type-member-plane` supersedes the old dual-plane premise; `@dot-brace-nesting` remains the surface path baseline.
