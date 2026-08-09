# option(T): absence as a value, never NULL (design, user-approved direction 2026-08-08)

User word: "for null, should we make optional a special enum? idk how to treat
this but 3vl does suck ass" -> direction: yes, a blessed enum constructor.

## TOC
- The decision
- The three existing spellings and when each wins
- Surface and desugar
- Printer mappings
- What stays banned
- Open rows for the ruling

## The decision

`option(T)` (candidate surface sugar: `t?` in column position) is a CLOSED type
constructor beside `json_list(T)`: one former, hardcoded instantiation set, no
general generics machinery. It desugars to the nullary-variant enum that
already works end to end (fixture receipt: maybe_text some/none, arrivals and
retractions, landed 2026-08-08 in the enum arc).

```
rel user(id: int, email: option(text));
   ==== desugars to ====
enum __opt_text { some(value: text) ; none() }
rel user(id: int, email: int);          % the option instance id, plain int
% reading a variant = ordinary join on the tag rel, same as every enum
```

## The three spellings stay legal; the sugar picks the enum

| spelling | absence is | wins when |
|---|---|---|
| split rel (`user_email(id, email)`) | a missing row | absence is default state; joins simply do not match |
| option(T) enum | a tagged row | absence must tick, retract, and differ from "never stated" |
| json hole | a missing key | the datum already lives in one json cell |

## Why 3VL cannot enter

The value plane never stores NULL. Absence is either no row (nothing to
compare) or a `none` tag (an ordinary value comparing true/false). Guards stay
two-valued by construction. Emitted-SQL internals (LEFT JOIN scaffolding) may
hold transient NULLs; the boundary decode owns that seam and the flip
campaign's json-hole family already gates it.

## Printer mappings (one dialect clause per target, registry pattern)

| target | option(text) spells as |
|---|---|
| sqlite storage | the instance id column, INTEGER; tag rel as today |
| ts boundary | `string \| undefined` — a union, never `null` |
| rust (kimi step d) | `Option<String>` |
| jsonschema (steps e/h) | property absent from `required` |

## What stays banned

- `option(T)` in a KEY column: identity with optional parts reopens SQL's
  null-in-PK swamp. Named check error.
- NULL as a literal anywhere in the surface. No spelling exists; keep it so.
- 3VL operators (`IS NULL` analogues). Absence is queried by join or by tag.

## Open rows for the user ruling
1. Surface spelling: `option(text)` vs `text?` vs both.
2. Does `none` compare equal across DIFFERENT option instances (shared none
   row vs per-instance)? Proposal: per-instance, ids never cross types.
3. Whether the desugar mints one enum per element type (`__opt_text`) or one
   per column site. Proposal: per element type, dictionary-style reuse.
