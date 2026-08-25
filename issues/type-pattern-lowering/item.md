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
- intent:lowering
- size:large
- model:large
size: L
lane: typegraph-core
lane_seq: 40
collision: [generic-type-core, compiler-oracle]
blocked_by: ['@typegraph-integration-plan', '@typegraph-node-edge-view']
closed: 2026-08-24
closed_by: codex
commits:
- hash: 1ab6c6cce
  summary: compiler lower structural type patterns
---

# Lower functional type patterns in compiler rule heads

## Description

Add safe structural construction and matching for type-node patterns. The motivating form is `serializable(primitive(Name))`; variable-bearing bare facts remain constrained by Datalog safety.

## Required Lowering

A head pattern lowers to explicit node construction or lookup goals. A body pattern lowers to explicit node-shape matching. Constructor applications such as `option(T)` continue through `type_apply` and remain distinct from canonical node tags.

## Safety

- Head variables are bound by positive body goals before construction.
- Body patterns bind fields from finite relation rows.
- Open bare facts remain unsafe unless ground.
- Recursive construction retains bounded refreeze and cycle checks.

## Acceptance Criteria

- [x] Head construction and body matching lower to relational goals.
- [x] Primitive, named, application, member-label, and variant-label shapes have tests.
- [x] `type_apply` remains canonical and separate from node matching.
- [x] Unsafe facts and non-ground construction have named diagnostics.
- [x] Runtime lowering either shares the mechanism or names the compile-time boundary.

## Tests Run

Parser, compiler safety, recursive construction, and lowerer tests.

## Implementation Notes

Execution tier: Large, size `L`, label `size:large`. Current Codex performs this compiler-semantic work directly after the integration plan selects the representation.

## Resolution

### 2026-08-25T03:11:05Z · @codex

Structural primitive, named, application, member, and variant patterns lower to finite compiler sources. Surface constructor heads retain type_apply construction; body patterns use canonical lookup. Runtime structural terms remain runtime data. Focused compiler_relations and compiler_type_graph gate passed 47/47.
