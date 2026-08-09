# Constraints as decorators + decl-spread instead of `&` (design sketch, PARKED)

STATUS (user word 2026-08-09): "no decorators/constraints yet please, avoid
those, do not touch at symbol." NOTHING in this doc is a go. The `@` symbol
stays unclaimed in the grammar; no lane implements any of it. This file is
future reference only.

User words: "constraints we borrow from typespec and allow meta kwarg
functional decorators/functors" and, on intersection: "we have | do we have
to have &?" -> answer taken: no `&`; spread at declaration covers allOf's
real use.

## TOC
- Decorators: the third registry-row application
- Decl-spread: composition without intersection types
- openapi mapping both directions
- Open rows for ruling

## Decorators

```
rel user(id: int, email: text @maxLength(320) @format("email"));
```

- Surface: `@name(args...)` after a column type, zero or more, order kept.
- Storage: constraint rows in the registry (name, arity, per-target
  spellings), the same pattern as expression/5 for functions and the dialect
  maps for types. A decorator is DATA; targets implement incrementally; a
  missing spelling refuses at print with a named term.
- Enforcement, two doors:
  1. DDL: the SQLite CHECK spelling (`CHECK (length(email) <= 320)`).
  2. Arrival door: reject carries a named term so a bad row is an error
     value, never a silent drop (envelope-enum philosophy).
- Oracle: each row carries an oracle eval so conformance can diff engines.
- openapi print: decorator name maps 1-1 to the jsonschema keyword where one
  exists (maxLength, pattern, minimum, format...). Unknown-to-openapi
  decorators are omitted from that printer, never errors (print concerns are
  per-door).

## Decl-spread

```
struct Base { id: int, created: int }
struct Extended { ...Base, name: text }
```

- Expansion-time field copy (stage 1, same phase family as enum expansion);
  by the relational IR it is an ordinary struct. No subtyping, no variance,
  no `&` operator anywhere in the language.
- Conflict rule: a duplicated field name across spreads is a named error,
  no silent override (proposal; alternative last-wins is the json spread
  behavior — ruling row 3).

## openapi mapping

- EMIT: a struct built with spread prints as `allOf: [$ref Base, {props}]`;
  decorators print as their jsonschema keywords.
- IMPORT (step h): `allOf: [$ref X, {props}]` -> spread; every other allOf
  shape keeps its named refusal. Constraint keywords -> decorators.

## Future sugar, noted not needed (user, same session): `rel a(A & B)`

Would desugar to the natural join rule
`a(Id, AF..., BF...) <- A(Id, AF...), B(Id, BF...);` — columns union, rows
intersect, no new source of truth (maintained view; either side's retraction
retracts). The dual of spread: spread composes a SHAPE (new table,
independent rows), `&` composes FACTS about one entity (join on shared key,
compatible-keys check required). Datalog's `,` already is `&`; this is
declaration-position sugar only. Parked until a fixture wants it.

## Open rows for ruling
1. Decorator set v1: which TypeSpec names land first (maxLength, minLength,
   pattern, minimum, maximum, format?).
2. Do decorators attach to rel columns only, or also struct fields and enum
   variant fields?
3. Spread duplicate-field rule: named error vs last-wins.
4. Are decorators allowed on Key() columns (constraint on identity)?
