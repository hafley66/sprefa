---
created: 2026-08-18
updated: 2026-08-18
type: epic
owner: codex
status: done
priority: high
labels:
- area:dl6
- intent:type-system
closed: 2026-08-20
---

# Relational type schema

## Description

Plan: `plans/2026-08-18-relational-type-schema-wrappers-and-literals.md`

Unify type-valued compiler relations, transparent schema wrappers, functional type returns, and owner-scoped anonymous product/sum literals over one semantic type graph.

## Acceptance Criteria

- [x] Parameterized interface bounds retain their complete application through every typegen target.
- [x] Compiler relations use module-qualified recursive semantic type IDs.
- [x] `Self`, key, return, and anonymous-origin roles cross one target-independent semantic IR.
- [x] Type-valued facts and safe rules evaluate in the compiler and oracle, then erase before runtime planning.
- [x] `key(T)` reuses existing runtime key/storage lowering.
- [x] Anonymous products and sums have owner-scoped identity, construction/visibility contracts, and cross-target artifacts.
- [x] Rust trait generic versus associated-output emission follows declared relation cardinality.
- [x] Real-source cross-target CI compiles generated TypeScript and Rust and validates JSON Schema.

## Tests Run

2026-08-20 audit: `anonymous_type_syntax` 25/25 and `compiler_relations`
19/19. Child cards `@interface-bound-transport`, `@semantic-type-identity`,
`@type-relation-ir`, `@compiler-type-relations`,
`@key-wrapper-normalization`, `@anonymous-type-syntax`,
`@anonymous-product-values`, `@anonymous-sum-values`,
`@rust-associated-outputs`, and `@relational-type-ci` are closed with their
respective focused and cross-target evidence.

## Implementation Notes

Two Sol code-and-golden reviews were reconciled into the plan before card creation. The dependency DAG is authoritative; cards sharing compiler-core collision tokens do not run concurrently.
