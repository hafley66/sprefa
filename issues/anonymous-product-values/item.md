---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: luna
status: done
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@anonymous-type-syntax']
lane: anonymous-types
lane_seq: 1
collision: [storage-lowering, type-emitters]
closed: 2026-08-18
commits:
- hash: 01d70184c
  summary: Implement anonymous product runtime values
---

# Materialize anonymous product values

## Description

Mint owner-scoped product declarations after specialization, reuse relation-value storage, add contextual construction/matching or named schema-only refusals, and verify public TS/Rust/JSON/product runtime shape.

## Acceptance Criteria

- [x] Product literals mint ordinary owner-scoped product declarations after specialization.
- [x] Struct-plane storage retains one product-valued owner column without flattening.
- [x] Every externally referenced product receives reachable TS/Rust/JSON definitions.
- [x] Authored construction and matching execute, or receive the exact planned schema-only refusal.
- [x] Equivalent named and anonymous product forms have specified identity and value behavior.

## Tests Run

## Implementation Notes

Use `mint_anonymous_product(+OwnerTypeId,+SitePath,+OrderedFields,-ProductTypeId,-GeneratedDecls)`, with ordered fields retaining names and semantic types. Construction and matching are contextual: a product term is legal only where the expected column type identifies one product owner; otherwise refuse `anonymous_product_context_missing(Path)` or `anonymous_product_context_ambiguous(Path)`. Store the owner column as one relation endpoint and reuse existing struct-plane interning. Named and anonymous products with equal fields have equal value behavior and distinct nominal semantic identities. Reads follow the existing relation-valued column; writes intern the complete product before the owner row. Dense catalog IDs do not participate in anonymous identity.

## Decisions

### 2026-08-18T22:13:46Z · @codex

Target mapping: TypeScript may render a structural object or reachable alias; Go renders a generated Named over Struct; Rust renders a generated struct/Adt; SQLite renders the generated relation table plus endpoint ID. All target forms derive from one DL6 anonymous semantic identity.

## Comments

### 2026-08-19T00:17:28Z · @codex

CI: anonymous product 7/7, anonymous syntax 20/20, type relation IR 24/24. Independent review defects in wrapper descent and obj/1 carrier collision were corrected before integration.
