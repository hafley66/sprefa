---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: codex
status: in-progress
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

- [ ] Head construction and body matching lower to relational goals.
- [ ] Primitive, named, application, member-label, and variant-label shapes have tests.
- [ ] `type_apply` remains canonical and separate from node matching.
- [ ] Unsafe facts and non-ground construction have named diagnostics.
- [ ] Runtime lowering either shares the mechanism or names the compile-time boundary.

## Tests Run

Parser, compiler safety, recursive construction, and lowerer tests.

## Implementation Notes

Execution tier: Large, size `L`, label `size:large`. Current Codex performs this compiler-semantic work directly after the integration plan selects the representation.
