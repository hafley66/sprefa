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

## Optional REFERENCES: the other half (user word, same session: "yes i want
## optional rel's that is like the other half of it")

`option(<rel-ref>)` is SUPPORTED, with a DIFFERENT desugar than scalars: the
split rel. An optional reference is exactly the case where absence-as-missing-
row is the natural lowering, and it keeps ids out of the value plane (the
invariant that bans ids inside json stays intact).

```
rel commit(id: int, reviewed_by: option(person));
   ==== desugars to ====
rel commit(id: int) key(id);
rel commit__reviewed_by(commit_id: int, person_id: int) key(commit_id);
% absence = no row; presence = one row per commit (keyed), retractable
```

Reads are the join you would write by hand; `none` never exists as a value:

```
reviewed(CommitId, PersonName) <-
  commit(CommitId), commit__reviewed_by(CommitId, PersonId),
  person(PersonId, PersonName);
```

So the one surface `option(T)` picks its lowering by T: scalar -> the
some/none enum instance; rel reference -> the companion split rel. Both erase
NULL; both keep guards two-valued.

## What stays banned

- `option(T)` in a KEY column: identity with optional parts reopens SQL's
  null-in-PK swamp. Named check error.
- NULL as a literal anywhere in the surface. No spelling exists; keep it so.
- 3VL operators (`IS NULL` analogues). Absence is queried by join or by tag.
- option instance ids inside json values (unchanged invariant; the ref desugar
  above is how optional relationships avoid it).

## Open rows for the user ruling
1. Surface spelling: **RULED both legal** (user 2026-08-09, rulings.pl option_surface).
2. `none` identity: **RULED per-instance** (user 2026-08-09), ids never cross types.
3. Enum minting: **RULED per element type** (user 2026-08-09), `__opt_text` style, dictionary reuse.
