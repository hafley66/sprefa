---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: luna
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@semantic-type-identity']
lane: type-core
lane_seq: 2
collision: [generic-type-core, catalog-schema]
closed: 2026-08-18
---

# Add semantic member and type-relation IR

## Description

Add target-independent schema_member and type_relation rows carrying self_subject, key, return, authored type, anonymous origin, and member roles through catalog/typegen transport.

## Acceptance Criteria

- [x] `schema_member` retains authored type, semantic value type, position, and roles.
- [x] `type_relation` retains `Self`, inputs, return, and key members.
- [x] Exactly one first `Self: type` member is accepted for trait-like relations; malformed cases are named refusals.
- [x] Catalog and typegen transport preserve roles without relying on names.
- [x] Ordinary runtime relation artifacts remain unchanged when no new roles occur.

## Tests Run

## Implementation Notes

Authoritative seams: `v6/prolog/0_generic_expand.pl`, `v6/prolog/0_type_ids.pl`, `v6/prolog/compile/lower.pl`, and `v6/prolog/compile/typegen_export.pl`. Add normalized rows shaped as `schema_member(MemberId, OwnerTypeId, Position, Name, AuthoredType, ValueTypeId, Roles)` and `type_relation(OwnerTypeId, SelfMemberId, InputMemberIds, ReturnMemberOrNone, KeyMemberIds)`. `Roles` is an ordered deduplicated list drawn from `self_subject`, `key`, `return`, and `anonymous_owner(Path)`. Owner plus member position is unique. Exactly one first `Self: type` is required only when a relation is projected as a trait. Refusals: `type_relation_self_missing/1`, `type_relation_self_duplicate/1`, `type_relation_self_not_first/1`, and `type_relation_self_not_type/2`. Rust emitters consume `self_subject`; they never render `Self` as an ordinary field.

## Comments

### 2026-08-18T20:26:59Z · @codex

CI: focused 13/13 passed; full compiler suite retained the same six current-main failures. Review corrected trait classification, replaced Prolog-list JSON freight with typed child relations and SHA IDs, scoped Rust Self suppression through column/member/role joins, and repaired four false-positive refusal tests.
