---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: terra
status: done
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@semantic-type-identity', '@type-relation-ir']
lane: anonymous-types
lane_seq: 0
collision: [parser-type-expr, generic-type-core]
closed: 2026-08-18
commits:
- hash: 68c3ad28b
  summary: 'dl6: anonymous product/sum type syntax and identity'
---

# Add anonymous type syntax and identity

## Description

Add recursive product/sum type AST, printer, Tree-sitter CST, module-resolved owner-site identity, full named enum payload type expressions, reachability metadata, and recursion diagnostics. Resolve contextual product construction contract.

## Acceptance Criteria

- [x] Product and sum literals parse in every type-expression position and reach a parse/print/reparse fixpoint.
- [x] Tree-sitter grammar and node types match the Prolog surface.
- [x] Sum payloads are named fields whose types may be complete type expressions.
- [x] Owner identity is assigned after module resolution and includes recursive site path.
- [x] Generic substitution precedes anonymous ID minting; enum context sees minted sums.
- [x] Product, enum, mixed, and recursive-cycle behavior has named outcomes.
- [x] Contextual construction/matching or schema-only refusal is decided and documented.

## Tests Run

## Implementation Notes

AST terms are `product_type([field(Name,TypeExpr),...])` and `sum_type([variant(Name,[field(Name,TypeExpr),...]),...])`. Empty products and sums receive named refusals in the first slice. Sum payloads remain named. Add these terms to the Prolog parser/printer and Tree-sitter `type_expr`; verify second-print byte equality. Mint after module resolution and concrete generic substitution as `anonymous(OwnerSemanticTypeId, SitePath, Shape)`, where `SitePath` is the recursive sequence of member names plus wrapper/application argument ordinals. Unrelated declaration insertion cannot change it. Product recursion follows the existing relation-value DAG refusal; enum recursion uses the existing guarded enum behavior; mixed unguarded cycles receive `anonymous_type_cycle(Owner,Path)`. Contextual product construction and matching are included, using the expected column type to select the anonymous owner. Generated visibility uses anonymous kind/origin plus reachability, not `__` substring filtering.

## Decisions

### 2026-08-18T22:13:46Z · @codex

Anonymous product syntax is a source-level elision only. After module resolution and concrete generic specialization, mint owner-scoped identity anonymous(OwnerSemanticTypeId, SitePath, SpecializedShape), then materialize an ordinary generated type_decl before relation-value, option, enum, and storage lowering.
