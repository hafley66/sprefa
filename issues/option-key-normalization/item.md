---
created: 2026-08-18
updated: 2026-08-19
type: task
assignee: luna
status: done
closed: 2026-08-19
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:storage
- codex
lane: storage
lane_seq: 1
collision: [storage-lowering, option-lowering]
blocked_by: ['@key-wrapper-normalization']
---

# Define option values as relation keys

## Description

Replace the temporary option_in_key_column refusal with one canonical key identity for none and some(value) across enum-backed scalar options and companion-relation options.

## Acceptance Criteria

- [x] Define one portable `Key<option(T)>` representation for `none` and `some(ValueKey)`.
- [x] Preserve distinct `none`, nested `some(none)`, and `some(value)` states where the element type permits them.
- [x] Scalar, enum, and relation option storage implement the same content-interned key equality.
- [x] TypeScript, Rust, SQLite, compiler oracle, and both runtime engines agree on normalization.
- [x] Composite keys containing options replace and retract deterministically across restart.
- [x] Remove `option_in_key_column` and its unsupported fixture.

## Tests Run

## Implementation Notes

Keyed options remain in the owner row as a canonical enum endpoint. The durable
enum identity table maps canonical tagged JSON to a dense integer. SQLite sees
only that integer and never receives SQL `NULL`. Equal `none` or `some(value)`
values therefore share key identity across ticks and restarts.

## Comments

### 2026-08-19T13:01:47Z · @codex

Runtime audit found the required primitive: EnumPlane derives enum endpoint identity from a sibling owner id column. A key(option(T)) column may be the only column, so structured TS/Rust ingress currently throws ambiguous_owner_context even though raw integer schedules pass. Completion requires content-interned enum identity shared across variants, then option keys can store that integer. Provisional refusal removal was reverted.
