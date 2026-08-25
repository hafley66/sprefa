---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: codex
status: done
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
closed: 2026-08-25
closed_by: codex
commits:
- hash: ea841689b
  summary: derive type projection in DL6
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

The library declares `type.project/3` as a compiler relation with key `(Owner, Name)`. Its authored DL6 rules fill it during the compiler fixpoint. Surface dotted-name parsing remains in the path resolver; this card replaces the semantic projection oracle used by compiler rules.

## Acceptance Criteria

- [x] Projection logic is authored DL6 rather than `5a_type_projection.pl`.
- [x] Members, nested relations, and approved variants share one lookup relation.
- [x] Physical storage rows never enter the semantic lookup relation.
- [x] Deep dots resolve in every reference-bearing position.
- [x] Existing conflict, deduplication, inline-sum, and diagnostic receipts remain exact.

## Tests Run

- Projection library: 3/3 passed.
- Compiler relations, type graph, projection, and braced nesting: 92/92 passed.
- Anonymous type, anonymous product, projection, and braced nesting regression gate: 66/66 passed.
- Complete Prolog suite: 1112/1112 passed in 18 seconds.
- `git diff --check`: passed.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Implemented directly in an isolated worktree. `@remove-type-member-plane` supersedes the old dual-plane premise; `@dot-brace-nesting` remains the surface path baseline. Implementation commit: `ea841689b`.

## Resolution

### 2026-08-25T13:27:15Z · @codex

Declared keyed type.project/3 in DL6, derived canonical member, variant, and nested projections, removed the host source oracle, retained exact path diagnostics, and passed 1112 complete Prolog tests.
