---
created: 2026-08-18
updated: 2026-08-18
type: epic
owner: codex
status: open
priority: high
labels:
- area:dl6
- intent:type-system
---

# Relational type schema

## Description

Plan: `plans/2026-08-18-relational-type-schema-wrappers-and-literals.md`

Unify type-valued compiler relations, transparent schema wrappers, functional type returns, and owner-scoped anonymous product/sum literals over one semantic type graph.

## Acceptance Criteria

- [ ] Parameterized interface bounds retain their complete application through every typegen target.
- [ ] Compiler relations use module-qualified recursive semantic type IDs.
- [ ] `Self`, key, return, and anonymous-origin roles cross one target-independent semantic IR.
- [ ] Type-valued facts and safe rules evaluate in the compiler and oracle, then erase before runtime planning.
- [ ] `key(T)` reuses existing runtime key/storage lowering.
- [ ] Anonymous products and sums have owner-scoped identity, construction/visibility contracts, and cross-target artifacts.
- [ ] Rust trait generic versus associated-output emission follows declared relation cardinality.
- [ ] Real-source cross-target CI compiles generated TypeScript and Rust and validates JSON Schema.

## Tests Run

## Implementation Notes

Two Sol code-and-golden reviews were reconciled into the plan before card creation. The dependency DAG is authoritative; cards sharing compiler-core collision tokens do not run concurrently.
