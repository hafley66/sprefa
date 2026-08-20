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
- intent:storage
blocked_by: ['@type-relation-ir']
lane: storage
lane_seq: 0
collision: [generic-type-core, storage-lowering]
closed: 2026-08-18
---

# Normalize transparent key wrappers

## Description

Normalize lowercase key(T) after generic substitution and before option/mirror/storage lowering. Reuse keyed/2 and rel/5. Cover composite order, legacy conflict, nested/repeated/option refusals, replacement, and stale-row retraction.

## Acceptance Criteria

- [x] `key(T)` parses as an ordinary application and normalizes to underlying `T` plus a key member role.
- [x] Normalization runs after specialization and before option, mirror, storage, and rel-plan lowering.
- [x] Wrapper and legacy positional keys produce identical `keyed/2`, `rel/5`, DDL, upsert, and replacement behavior.
- [x] Composite order follows relation-column order.
- [x] Differing legacy/wrapper keys and nested, repeated, or optional key wrappers receive named refusals.
- [x] Replacement followed by stale-row retraction is deterministic.

## Tests Run

## Implementation Notes

Authoritative seams: `v6/prolog/1_expansion.pl`, `v6/prolog/0_generic_expand.pl`, `v6/prolog/0_option_expand.pl`, `v6/prolog/0_type_plane.pl`, and `v6/prolog/0_rel_record.pl`. Add `normalize_key_wrappers(+Decls0,-Decls)` after concrete specialization and before option expansion. For each outer `col_type(Ref, Position, key(T))`, emit `col_type(Ref, Position, T)`, member role `key`, and one canonical `keyed(Ref, OrderedPositions)`. Refusals: `key_wrapper_nested(Ref,Position)`, `key_wrapper_repeated(Ref,Position)`, existing `option_in_key_column`, and `key_wrapper_legacy_conflict(Ref,WrapperPositions,LegacyPositions)`. Identical wrapper and legacy positions deduplicate. Runtime write sequence remains existing SQLite keyed upsert. Retraction matches the complete stale row and must not remove the replacement row. Do not add `key` blindly to runtime value-wrapper traversal.

## Comments

### 2026-08-18T22:58:57Z · @codex

CI: focused 24/24 passed. Independent review found no normalization defect. Follow-up added exact wrapper/legacy plan and lowering parity plus executed add-old, add-replacement, retract-old timeline proving replacement retention.
