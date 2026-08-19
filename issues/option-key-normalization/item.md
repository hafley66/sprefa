---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: luna
status: open
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:storage
lane: storage
lane_seq: 1
collision: [storage-lowering, option-lowering]
blocked_by: ['@key-wrapper-normalization']
---

# Define option values as relation keys

## Description

Replace the temporary option_in_key_column refusal with one canonical key identity for none and some(value) across enum-backed scalar options and companion-relation options.

## Acceptance Criteria

- [ ] Define one portable `Key<option(T)>` representation for `none` and `some(ValueKey)`.
- [ ] Preserve distinct `none`, `some(null-like)`, and `some(value)` states where the element type permits them.
- [ ] Scalar/enum-backed and relation-companion option storage implement the same key equality.
- [ ] TypeScript, Rust, SQLite, compiler oracle, and both runtime engines agree on normalization.
- [ ] Composite keys containing options replace and retract deterministically across restart.
- [ ] Remove `option_in_key_column` and its unsupported fixture only after the positive runtime cases pass.

## Tests Run

## Implementation Notes

This is an implementation-deferred refusal. The current parent-column key path requires a directly stored value, while relation options encode `none` as absence of a companion row. Do not select SQLite `NULL` implicitly. Specify the portable key and then project each storage representation onto it.
