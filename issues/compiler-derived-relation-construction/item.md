---
created: 2026-08-23
updated: 2026-08-23
type: feature
status: done
priority: high
labels: [compiler, types]
closed: 2026-08-23
---

# Compiler-derived relation construction

## Description

Implement the reviewed plan in
`plans/2026-08-23-compiler-derived-relation-construction.md`.

## Acceptance Criteria

- [x] Functional type terms in compiler heads lower to explicit type_apply goals.
- [x] Demand-driven derived relation requests validate and materialize through bounded refreeze.
- [x] Partial(User) produces canonical optional fields without copied key roles.
- [x] Generated relation output reaches existing target lowering.
- [x] Compiler request transport erases before runtime planning.

## Tests Run

- [x] Focused compiler relation tests
- [x] Complete Prolog compiler suite
- [x] Cross-target generated artifact gates

## Implementation Notes

- Plan: `plans/2026-08-23-compiler-derived-relation-construction.md`
- Functional head terms lower inside-out to `type_apply/3`.
- Validated shape rows become ordinary relation carriers for the next refreeze
  round.
- Derived applications rewrite authored storage types to their canonical
  generated relation names.
- Catalog metadata marks derived materializations as concrete target types.
- `0_generic_expand.pl` is a module facade over numbered implementation files.

## Resolution

### 2026-08-23T19:38:31Z · @issuectl

Implemented functional type-head lowering, demand-driven request validation, bounded materialization/refreeze, canonical storage rewriting, concrete target metadata, Partial(User), nested deduplication, and cross-target snapshots. CI: 37 compiler relation tests; complete PLUnit 1045 declared/1091 passed; typegen golden holds across TS, Rust, JSON Schema, and runtime gates.
