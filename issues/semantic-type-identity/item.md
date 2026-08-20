---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: terra
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
lane: type-core
lane_seq: 1
collision: [generic-type-core]
blocked_by: ['@interface-bound-transport']
closed: 2026-08-18
---

# Define semantic type identity domains

## Description

Define and implement module-qualified recursive SemanticTypeId. Keep SemanticTypeId, CatalogRowId, and RuntimeEndpointId distinct. Add determinism, module collision, and application identity tests.

## Acceptance Criteria

- [x] Named IDs contain module identity, declaration kind, and local name.
- [x] Application IDs recursively contain constructor and ordered semantic argument IDs.
- [x] Anonymous owner-site IDs have a specified recursive path and insertion-stability policy.
- [x] Catalog row and runtime endpoint IDs cannot enter compiler type relations.
- [x] Reordering unrelated declarations preserves semantic IDs; same names in separate modules do not collide.

## Tests Run

## Implementation Notes

Document conversion functions between semantic IDs, dense catalog IDs, and runtime endpoint IDs. No second type registry.

## Decisions

### 2026-08-18T19:03:54Z · @codex

Implementation contract: semantic IDs are ground Prolog terms named(ModuleHash, Kind, Name), primitive(Name), and application(ConstructorSemanticId, OrderedArgumentSemanticIds). Compiler equality uses terms; semantic_type_id_text/2 produces full SHA-256 text only at artifact boundaries. Add semantic_decl_module/3 before import merging. Keep CatalogRowId and RuntimeEndpointId unchanged and separate. Migration order: use_resolve.pl, 0_type_ids.pl, 0_generic_expand.pl, 0_enum_expand.pl, lower.pl, typegen_export.pl, then goldens.

## Comments

### 2026-08-18T19:31:49Z · @codex

CI: 164 focused identity/generic/import tests passed; compiler-produced generic typegen JSONL matched the checked-in Prolog TS golden. Full compiler suite ran 827 tests with 7 unrelated current-main failures. Review correction added exact UTF-8 byte-length SHA-256 vectors and a closed ground constructor check.
