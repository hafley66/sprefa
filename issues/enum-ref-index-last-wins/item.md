---
created: 2026-08-19
updated: 2026-08-19
type: bug
status: open
priority: normal
labels: [compiler, emit-ts]
---

# enum_ref_index/2 fold makes the LAST decl win per (Ref, Column)

## Description

`4e2c21a82` (perf: index large schema compilation) replaced a disjunction that
preferred `enum_column` over `option_column` with a left-to-right `put_assoc`
fold at `v6/prolog/emit_ts.pl:358-366`, so the last matching decl now wins per
`(Ref, Column)` key. Pokeapi output stayed byte-identical, so nothing bit yet.
Behavior change hiding inside a perf commit; found by the shared-frontier
revalidation lane (lab PR #376).

## Acceptance Criteria

- [ ] A fixture with both an `enum_column` and an `option_column` decl on the
  same `(Ref, Column)`, pinning which one the emitter reads.
- [ ] Either restore the enum-over-option preference in the fold or record the
  last-wins order as intended.
