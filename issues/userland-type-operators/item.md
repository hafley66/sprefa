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
- intent:type-system
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 50
collision: [generic-type-core]
blocked_by: ['@typegraph-node-edge-view', '@typegraph-member-planes', '@type-pattern-lowering', '@compiler-plane-expression-parity', '@compiler-stratified-negation']
closed: 2026-08-25
closed_by: codex
commits:
- hash: 894b8a917
  summary: userland type operators close over canonical rows
---

# Define impl concat inherit and extends as user-land relations

## Description

Prove common type operators can be ordinary DL6 libraries over canonical graph rows without compiler builtins.

## Required Demonstrators

`serializable(Type)`, `Partial(Type)`, `extends(Child, Parent)`, `impl(Type, Interface)`, and `concat(Left, Right, Output)`.

`Partial` derives optional logical members. `extends` and `impl` are graph relations. `serializable` recursively follows targets. `concat` derives generated members with collision diagnostics.

## Acceptance Criteria

- [x] Every demonstrator is DL6 library code.
- [x] `Partial(T)` materializes through bounded refreeze.
- [x] Recursive serializability has a cycle policy.
- [x] `extends` and `impl` support transitive queries.
- [x] `concat` preserves order and rejects incompatible names.
- [x] No higher-kinded feature is required.
- [x] New operators using existing rows require only DL6 code.

## Tests Run

Compiler fixpoint, generated relation, recursive graph, conflict diagnostic, and complete Prolog regression tests.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by node/edge rows, member planes, and structural patterns.

## Decisions

### 2026-08-25T05:05:08Z · @codex

Cycle policy: derive the finite candidate graph, seed serialization_blocked from candidate nodes lacking an approved shape, propagate blocked through member and application-argument edges by positive recursion, then derive serializable as the stratified complement. A recursive SCC is serializable when every reachable node and constructor has an approved shape. No depth counter or higher-kinded feature is involved.

## Agent Runs

### 2026-08-25T05:05:08Z · @codex

Implemented v6/dl/type/0_operators.dl6 with Partial, concat, extends, impl, and recursive serializability. Added generated-relation, member-role, transitive closure, recursion, custom operator, and collision coverage. Preserved ordered generated carrier rows during deduplication. Focused compiler-plane gate: 65/65. Complete Prolog gate: 1,111/1,111 in 16.9 seconds. No TypeScript suite run.

## Resolution

### 2026-08-25T05:05:35Z · @codex

Partial and concat materialize ordered generated relations; extends and impl close transitively; serializability handles recursive SCCs through positive blocked reachability plus stratified complement. Complete Prolog gate passes 1,111/1,111.
