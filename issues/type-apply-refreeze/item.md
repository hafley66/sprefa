---
created: 2026-08-22
updated: 2026-08-22
type: feature
assignee: terra
status: open
priority: high
epic: comptime-type-model
related: ['@semantic-type-identity', '@compiler-type-relations', '@type-relation-ir', '@type-annotation-eval', '@review-type-fixpoint', '@review-higher-kinds']
labels:
- area:dl6
- pkg:prolog
---

# Closed constructor application and bounded refreeze

## Description

Implement relation-shaped closed constructor application for compiler-time type relations. Reuse `application(ConstructorTypeId, OrderedArgumentTypeIds)` and existing generic, wrapper, enum, and anonymous minting. Preserve compiler-plane erasure.

## Acceptance Criteria

- [ ] Compiler IR represents constructor application as an interpreted body relation or explicit request relation without function-bearing rules.
- [ ] Existing ground applications reuse their canonical `SemanticTypeId`; absent applications enter a deduplicated next-construction frontier keyed by `application(ConstructorTypeId, OrderedArgumentTypeIds)`.
- [ ] Compiler rounds observe immutable canonical type-source rows, then refreeze generated declarations before the next query round.
- [ ] The outer loop stops on stable canonical type rows and emits reachable named diagnostics for arity, unknown constructor, non-ground application, recursive construction, and round-limit exhaustion.
- [ ] Existing generic, wrapper, enum, and anonymous specialization remains the minting authority; compiler rows, source views, requests, and evidence are absent from runtime relations and emitted storage.
- [ ] Focused compiler/type-relation/annotation tests and the repository full Prolog compiler CI pass with recorded receipts.
- [ ] Independent review against the fixed semantic contract is complete; any correction is independently verified.
- [ ] Implementation and issue receipts are committed with `Refs-Issue: @type-apply-refreeze`.

## Tests Run

Pending implementation.

## Implementation Notes

Closed named fixed-arity constructors only. Constructor variables, higher kinds, mixed runtime/comptime staging, unrestricted chase policies, and general rule generation remain outside this card.

## Comments

### 2026-08-22T21:59:22Z · @terra

Current focused CI: compiler_relations 28/28; type_relation_ir 57/57; annotation_surface 8/8; total 93/93. git diff --check passed. Independent review remains pending.
