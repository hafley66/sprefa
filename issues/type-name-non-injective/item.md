---
created: 2026-08-15
updated: 2026-08-15
type: bug
reporter: fable
status: open
priority: normal
labels:
- bugmine
- area:compiler
---

# type_name/2 is non-injective: camelCase and snake_case rel names collide on one TS/Rust type name

## Description

## Description

Metamorphic rename pass found: the rel-name -> TS/Rust type-name transform `type_name/2` is non-injective. Two different rel names can map to the same PascalCase type name, so the emitted `.types.ts` / `.types.rs` collide.

## Site

`compile/7_emit_ts_types.pl:172` and `compile/8_emit_rust_types.pl:172` — `type_name(Name, Type)` splits on `_`, drops empty parts, and upcases only the first character of each part. `capitalized/2` is at `7_emit_ts_types.pl:178`.

## Repro (smallest fixture)

`conformance/fixtures/10_list_elements.pl` `list_bare_column_round_trips` (single rel `box` -> `Box`). The collision is structural:

`type_name('foo_bar') = 'FooBar'` (split -> capitalize foo, bar)
`type_name('fooBar')  = 'FooBar'` (one part, upcase first char only)
`type_name('foo__bar')= 'FooBar'` (empty part dropped)

So a program with a camelCase rel `fooBar` beside a snake_case rel `foo_bar` emits two `interface FooBar` (or two `struct FooBar`) with no diagnostic. ALLCAPS and trailing-underscore names are also lossy: `REL_CAPS_3 -> RELCAPS3`, `rel_tail_2_ -> RelTail2`.

## Impact

This is the same name-sensitivity class PR #262 fixed for the `__dunder__` empty-part drop (the `exclude(empty_atom, ...)` filter) and for the SQL-side `module_type_stem`. The prolog `type_name/2` camelCase collision remains: the interior-capital / underscore structure is silently discarded rather than preserved.

## Decisions

### 2026-08-16T01:27:21Z · @fable

Boundary with the standing user decision: cross-module collisions resolve by MODULE PREFIX (CLAUDE.md; type_name/2 acknowledged non-injective there). This issue is the SAME-module case (foo_bar / fooBar / foo__bar in one module have no prefix to disambiguate). Any fix must not introduce numeric suffixes (decided against).
