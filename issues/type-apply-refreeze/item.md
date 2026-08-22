---
created: 2026-08-22
updated: 2026-08-22
type: feature
assignee: terra
status: done
priority: high
epic: comptime-type-model
related: ['@semantic-type-identity', '@compiler-type-relations', '@type-relation-ir', '@type-annotation-eval', '@review-type-fixpoint', '@review-higher-kinds']
labels:
- area:dl6
- pkg:prolog
closed: 2026-08-22
closed_by: terra
---

# Closed constructor application and bounded refreeze

## Description

Implement relation-shaped closed constructor application for compiler-time type relations. Reuse `application(ConstructorTypeId, OrderedArgumentTypeIds)` and existing generic, wrapper, enum, and anonymous minting. Preserve compiler-plane erasure.

## Acceptance Criteria

- [x] Compiler IR represents constructor application as an interpreted body relation or explicit request relation without function-bearing rules.
- [x] Existing ground applications reuse their canonical `SemanticTypeId`; absent applications enter a deduplicated next-construction frontier keyed by `application(ConstructorTypeId, OrderedArgumentTypeIds)`.
- [x] Compiler rounds observe immutable canonical type-source rows, then refreeze generated declarations before the next query round.
- [x] The outer loop stops on stable canonical type rows and emits reachable named diagnostics for arity, unknown constructor, non-ground application, recursive construction, and round-limit exhaustion.
- [x] Existing generic, wrapper, enum, and anonymous specialization remains the minting authority; compiler rows, source views, requests, and evidence are absent from runtime relations and emitted storage.
- [x] Focused compiler/type-relation/annotation tests and the repository full Prolog compiler CI pass with recorded receipts.
- [x] Independent review against the fixed semantic contract is complete; any correction is independently verified.
- [x] Implementation and issue receipts are committed with `Refs-Issue: @type-apply-refreeze`.

## Tests Run

Pending implementation.

## Implementation Notes

Closed named fixed-arity constructors only. Constructor variables, higher kinds, mixed runtime/comptime staging, unrestricted chase policies, and general rule generation remain outside this card.

## Comments

### 2026-08-22T21:59:22Z · @terra

Current focused CI: compiler_relations 28/28; type_relation_ir 57/57; annotation_surface 8/8; total 93/93. git diff --check passed. Independent review remains pending.

### 2026-08-22T22:03:25Z · @terra

Independent Luna review: one medium canonical-row termination finding, corrected in a3e857029; focused review receipt 93/93. Current full Prolog CI: just plunit completed 1,070 results, 1,069 passed, 1 failed: catalog_plane_rail:level_plane_family_corpus_counts at v6/prolog/compile/test/plunit_tests.pl:1845. Card remains open pending a passing full CI receipt.

### 2026-08-22T22:05:11Z · @terra

Luna review correction: a3e857029 added canonical semantic-row stability after request-driven continuation; the no-request fast path is committed separately. Focused CI: compiler_relations 28/28, type_relation_ir 57/57, annotation_surface 8/8, total 93/93. Full Prolog CI: just -f v6/Justfile plunit, declared 1024, results 1070, passed 1070, failed 0, timeout 0.

## Resolution

### 2026-08-22T22:05:11Z · @terra

Implementation, independent Luna review, focused CI, and full Prolog CI receipts recorded.
