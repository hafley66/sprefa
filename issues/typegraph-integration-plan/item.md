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
- intent:design
- size:large
- model:large
size: L
lane: typegraph-core
lane_seq: 0
collision: [generic-type-core, compiler-plans]
closed: 2026-08-24
closed_by: codex
---

# Freeze the user-land type graph integration plan

## Description

Produce the implementation plan reconciling the dirty temporal-v2 worktree, existing canonical type cards, and the user-land graph target. This card authorizes no feature code.

## Required Signatures

Price and select exact signatures for provisional node, edge, constraint, constraint-member, storage-name, structural construction, and structural matching relations.

## Required Analysis

1. Compare a generic edge view with specialized canonical rows plus adapters.
2. Compare structural semantic-term matching with explicit node-shape rows.
3. Define logical and storage member lifetimes.
4. Define constraint identity, grouping, ordering, and functional dependencies.
5. Define semantic path to physical storage-name mapping and companion-object collision rules.
6. Partition every hunk in `/private/tmp/sprefa-temporal-v2` into keep, move, replace, or discard.
7. Cross-reference existing reflection, storage, annotation, comptime-review, and dot-brace issues.

## Timeline and Storage

Show parse, module resolution, canonical freeze, compiler rounds, generated rows, target planning, SQL emission, and erasure. State creator, reader, lifetime, read/write order, and uniqueness for each row family.

## Acceptance Criteria

- [x] `plans/2026-08-24-userland-type-graph.md` gives type signatures followed by pseudocode bodies.
- [x] Instance lifetimes, storage layout, read/write sequence, and uniqueness are explicit.
- [x] Each architectural fork records both choices, evidence, and selected branch.
- [x] Existing issue overlap has no duplicate implementation ownership.
- [x] The dirty worktree has an exact commit and merge sequence.
- [x] Chris approves or explicitly defers disputed portions before implementation starts.

## Tests Run

Reconnaissance card. Record commands and counts in the plan.

## Implementation Notes

Execution tier: Large, size `L`, label `size:large`. Current Codex performs this directly. Any out-of-contract decision yields to Chris. A second large-model critique may review the plan markdown.

## Agent Runs

### 2026-08-24T22:22:38Z · @sol-xhigh

Sol xhigh read-only review on main `2c366a932`, 2026-08-24:

1. Canonical rows freeze and compiler relations evaluate before `program_plan/3` computes `AllRefs`, `RefColumns`, `RefTypes`, `Shapes`, and `RelPlans`. Inferred rule-head relations therefore have storage plans but cannot appear in compiler-time type queries.
2. Compiler-plane classification must remain early, while compiler evaluation and erasure move after preliminary runtime-shape inference and canonical completion.
3. Storage projection must consume a preliminary `RuntimeShapePlan` plus final target rows, then produce compatibility `rel/5`; consuming `RelPlans` creates a dependency cycle.
4. The DAG needs one new Large program-level comptime lifecycle card before node/edge, storage projection, member planes, pattern lowering, and temporal annotations.
5. Four user gates remain: source-stable versus world-sensitive inferred logical types; canonical path rows versus noncanonical path input; defining-module versus entry-module ownership for inferred rule-head relations; bounded scalar/order/aggregate compiler facilities now versus deferring `concat` and universal `serializable`.

The review also identified signature corrections for derivation identity, annotation keys, edge IDs, storage key authority, backend-scoped primary groups, key ordinal preservation, storage-name overrides, temporal FDs, and a separate `type.project` relation.

## Decisions

### 2026-08-24T23:14:04Z · @codex

Chris approved the active user-land type-graph plan on 2026-08-24 with one exclusion: an undeclared rule-head IDB remains a runtime relation under `Name/Arity` and does not receive a canonical `TypeId`, members, or compiler type rows. Authored declarations and compiler-generated declarations remain the type-graph domain. Future inferred IDB reflection is tracked by `@inferred-idb-type-reflection` and blocks no active card.

## Resolution

### 2026-08-24T23:14:21Z · @codex

Plan approved with undeclared IDB type reflection deferred. Signatures, lifetimes, storage sequence, forks, issue ownership, and temporal worktree partition are recorded.
