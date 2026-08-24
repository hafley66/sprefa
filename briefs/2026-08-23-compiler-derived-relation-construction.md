# Terra brief: compiler-derived relation construction

Implement the reviewed plan in
`plans/2026-08-23-compiler-derived-relation-construction.md` at xhigh effort.
Read the complete plan before editing.

## Objective

Add the compiler-only seam which turns one complete set of compiler-derived
relation member rows into one canonical materialized relation type, then prove
`Partial(User)` through bounded refreeze and existing target lowering.

Target flow:

```text
Partial(User)
  -> functional type term lowers to type_apply
  -> compiler rules derive a complete relation request
  -> request validates after compiler closure
  -> existing carrier and freeze pipeline materializes canonical rows
  -> SQLite, TypeScript, Rust, JSON Schema, and catalog lower normally
```

## Scope

- Signature-directed lowering of compound terms in `type`-typed compiler head
  columns into explicit `type_apply/3` body goals.
- Literal type-returning compiler relations as fixed-arity derived
  constructors.
- Demand-driven application projection through `type_requested/3`.
- Complete derived relation request header, member, and member-role relations.
- Deterministic request grouping, validation, diagnostics, and set
  deduplication.
- Materialization through existing declaration carriers and
  `freeze_type_rows/2`.
- Application-owned semantic-value field reflection through `type_field/5`,
  `type_field_count/2`, and the existing `derived_from/2` materialization edge.
- A `Partial(User)` fixture and complete cross-target proof.
- Compiler transport erasure and byte-identical no-request behavior.

## Boundaries

- No history, event, reducer, retention, or runtime-clock work.
- No higher-kinded constructor variables, kind signatures, partial
  application, or constructor currying.
- No new parser keyword, decorator, annotation sigil, or second type language.
- No runtime schema registry or schema mutation.
- No direct writes into frozen canonical member rows.
- Preserve structural application identity and existing generated declaration
  materialization identity.
- Preserve unrelated dirty-worktree files and user changes.

## Required implementation order

1. Record fail-first tests for functional head lowering, request rows,
   canonical rows, reflection, erasure, and cross-target output.
2. Implement functional type-head lowering while preserving explicit
   `type_apply/3` behavior and diagnostics.
3. Add demand projection, request relation signatures, and complete
   deterministic validation.
4. Factor or reuse generated relation carrier minting.
5. Feed validated requests into the existing bounded refreeze frontier.
6. Add the semantic-owner `type_field/5` and `type_field_count/2` projections.
7. Land the `Partial(User)` fixture and cross-target snapshots.
8. Run focused suites, then complete Prolog compiler CI and target gates.

## Files to inspect first

- `v6/prolog/0_type_ids.pl`
- `v6/prolog/0_compiler_relations.pl`
- `v6/prolog/0_generic_expand.pl`
- `v6/prolog/0_anonymous_expand.pl`
- `v6/prolog/compile/parse_dl_dcg.pl`
- `v6/prolog/compile/test/compiler_relations.test.pl`
- `v6/prolog/compile/test/type_relation_ir.test.pl`
- `issues/semantic-type-identity/item.md`
- `issues/type-apply-refreeze/item.md`
- `issues/review-type-fixpoint/item.md`
- `issues/review-higher-kinds/item.md`

## Tracking and handoff

- Use `issuectl` for one implementation card named
  `compiler-derived-relation-construction` if the card does not exist at
  dispatch time.
- Update acceptance checks and CI receipts as each phase closes.
- Commit implementation and tests in coherent phase commits.
- Report the exact changed file set, focused test counts, complete compiler CI,
  cross-target results, and any plan deviation.
- Stop and hail the parent before changing structural application identity,
  generated declaration identity, the 16-round policy, compiler/runtime phase
  boundaries, or authored syntax beyond the plan.
