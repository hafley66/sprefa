# BRIEF: pass 2. rel-as-interface, conformance links, and generic rel args.

> Historical brief. `issues/remove-rel-is/item.md` removed the relation
> conformance suffix used below on 2026-08-23.

## Base
Confirm the base with `git log --oneline -1` before your first commit. The spawn
printed the sha; that is your base. The ordering is not a gate. If a procedural
line in this brief seems to forbid otherwise-correct work, the work wins: note
the conflict in your report and keep going.

**Docs only. Write ZERO implementation code.**

## What pass 1 already delivered. Read it FIRST, extend it, do not redo it.

`plans/2026-08-12-typespec-module-ir.RESEARCH.md` (147 lines) and its
`.visual.human.unga.md` twin (180 lines), committed as `c57a06da`. They cover
TypeSpec 1.15.0 across the six fenced features and price 13 forks against this
repo's own emitter line counts. Pass 1's best finding, which you must not
contradict without evidence: a strongly connected component has no total
topological order, so deterministic emission order holds only after
condensation.

You are APPENDING to those two files, not creating new ones.

## The user has lifted a park

The standing note was: generics need a written inspection in docs before any
generics work, "no dont go chasing that yet but note we need to inspect how to
make generics as part of docs". The user has now said: **"its now open discussion
to finish the codegen dreams"**. So generics are in scope for DESIGN. Still no
implementation.

## Probed facts. Reproduce each one before building on it.

All probed 2026-08-12 through `compile_dl6/3`:

| probe | result |
|---|---|
| `rel point(x: int, y: int). rel line(a: point, b: point).` | **rc=0**. A rel used as a column type is legal beyond enums. |
| the DDL that produces | `CREATE TABLE "point" ("__id" INTEGER PRIMARY KEY, "x" INTEGER NOT NULL, "y" INTEGER NOT NULL, UNIQUE ("x","y"))` and `CREATE TABLE "line" ("__id" INTEGER PRIMARY KEY, "a" INTEGER NOT NULL, "b" INTEGER NOT NULL, UNIQUE ("a","b"))` |
| so a rel-typed column is | an INTEGER foreign key to the referent's `__id`, the surrogate-key law applied automatically |
| `rel body(page(view: text) ; redirect(to: text)). rel response(id: int, payload: body).` | rc=0, emits `body_page`, `body_redirect`, `body_tag`, and `response.payload` holds the enum id |
| `rel pair(T)(first: T, second: T).` | parse error at line 1 column 12, the second paren. No generic surface exists. |

And a phase that already exists: `v6/prolog/0_generic_expand.pl`, 348 lines. Its
header says "Generic expansion closes schema templates before enum expansion.
The artifact table uses typed records. Round one emits declarations only. Rules
remain author-written." It exports `canonical_type_name/2`,
`canonical_type_encoding/2`, `generic_artifact_order/3`. That is
monomorphization running as a compile phase TODAY for internal wrappers.

Read that file properly. The central question for the generics section is how
much of user-facing generics is surface syntax plus a route into this existing
phase, versus genuinely new machinery. Answer it with line counts.

Also relevant, already known: `option(<enum>)` is stopped at
`v6/prolog/0_option_expand.pl:43` and that is a PHASE ORDER accident, not a
design. `1_expansion.pl:28-29` runs option at phase 5 and enum at phase 10.

## Section A: what `rel` already means

Establish and verify this table, correcting anything wrong:

| what the declaration carries | role | storage |
|---|---|---|
| columns + `key(...)` | keyed stored rel | its own table |
| columns only | set rel | `("__id" PK, cols, UNIQUE(cols))` |
| variants `(a(...) ; b(...))` | closed sum type | variant tables plus a `_tag` |
| referenced as a column type | reference | none of its own; the column is INTEGER |
| rules | derived rel | its own table |

The user's observation is that `rel` is already overloaded as module, type, enum
and table, and that interface is the missing role. Say whether that reading is
right.

## Section B: rel-as-interface

**Interface is the one role that needs a new discriminator, because every
non-enum rel gets a table today and an interface must have no instances.**

The candidate framing to evaluate, offered as a starting point:

> An enum is a CLOSED sum declared at the type. An interface is the SAME GRAPH
> declared from the other end, an OPEN sum where each member declares itself.

If that holds, the lowering is the enum lowering with an open variant set: an
enum emits `body_tag`, an interface emits `addressable_tag(id, which_rel)`.
Test that claim against the actual enum lowering code and say whether it
survives. If it does, the emitter cost is small and you should say how small.

Answer:
- what marks a rel as an interface: an explicit keyword, or inference from being
  conformed-to and never populated. Price both. Inference means a declaration's
  meaning depends on the whole program, which is a real cost.
- whether an interface may carry anything beyond columns. **Flag from the
  coordinator: if it carries a RULE it becomes a default method, and Rust
  coherence, Go's lack of default methods, and TypeScript structural typing stop
  agreeing.** Say whether columns-only is the right floor.
- what an interface means for a DERIVED rel versus a stored one.

## Section C: the conformance link. The user asked for this directly.

> "anyone who implements an interface would need something written to say this
> rel links to this rel / implement this rel no?"

Yes, and note what already exists: `.dl6` has exactly ONE written rel-to-rel
link today, the has-a link of a column typed by another rel, lowering to an
INTEGER foreign key. Conformance is an is-a link and is a SECOND link kind.

- `<-` is the rule arrow and cannot be reused. Propose spellings and pick a
  recommended one, with the parse consequences of each.
- structural conformance (the rel has the columns) versus declared conformance
  (the rel says so). Table the trade: Go and TypeScript are structural, Rust
  traits are nominal with explicit impls. The user's phrasing leans declared.
  Price both anyway.
- does `implements` become a row in the type IR? The `plans/scip-as-ir` lane is
  examining SCIP's `is_implementation` and `is_type_definition` relationship
  kinds from the other direction. Consider adopting that spelling rather than
  minting a new one, and say why or why not.
- what the emitted Rust `impl` block looks like and who generates it.
- what happens when a rel conforms to two interfaces with a colliding column.

## Section C2: the three levels. Establish this ordering before section D.

The user hit a chicken-and-egg while reaching for the syntax and named it:
"ah shit we need generics on rels first before we can say oh wait wut i've gone
cross eyed". Resolve it explicitly in the doc, because anyone reading will hit
the same wall.

```
level 0   interface + conformance, NO generics needed
          rel addressable(path: text, digest: text).
          rel file(path: text, digest: text, bytes: int) is addressable.

level 1   generic rels as types
          rel pair(T)(first: T, second: T).
          rel coords(p: pair(int)).

level 2   generic interfaces, conformance to an instantiation
          rel container(T)(items: json_list(T)).
          rel tag_bag(items: json_list(text)) is container(text).
```

Two constructs look alike and are not. Table this distinction plainly:

| written | what it is |
|---|---|
| `pair(int)` | generic INSTANTIATION: mints a new concrete type, expanded at compile time into a table |
| `file is addressable` | CONFORMANCE: a claim about a rel that already exists, expanded into a tag row |

Also state plainly that there is NO impl block and nothing named like `my_impl`
is ever declared. Conformance is a clause on the rel that already exists. The
user reached for an impl entity and stalled; that is a Rust habit and the doc
should close it.

Verify that level 0 is genuinely independent of levels 1 and 2, and say so as a
staging recommendation with a price for each level.

## Section D: generic rel args

**The user has stated a preference: the CURRIED form.** Their words: "heavy
agree to the curry syntax, its that or we have left most column be the generic
decls but i would rather symmetrically curry it". So `rel pair(T)(first: T,
second: T)` is the leading candidate and the leftmost-columns alternative is
the one it beat. Design against that preference; if you find a blocking reason
it cannot work, that is a finding and it goes at the top of the section.

Two properties of the curried form the doc should confirm or refute:
- arity is unambiguous at parse time, no lookahead needed to tell type
  parameters from columns
- `rel point(x: int, y: int)` is the zero-argument case of the same rule, so
  every existing declaration keeps its shape

The Zig comparison is the user's own reference and it is apt: Zig spells a
generic `fn Pair(comptime T: type) type`, a function from types to types
evaluated at compile time. The curried rel form is the same idea; `pair` is not
a rel, `pair(int)` is. Use that framing if it helps the reader, and say where
the analogy stops.

- show the grammar change for the curried form. The current parse error is at
  line 1 column 12, the second paren.
- monomorphization is the fit, and the user named why: "that is symmetric with
  comptime goals". `pair(int)` and `pair(text)` become two tables; there is no
  polymorphic table with a runtime type column. Confirm this is what
  `0_generic_expand.pl` already does and say what changes for user-facing
  templates.
- the combinatorial cost: N templates times M instantiations is a table count.
  Give a real number from the corpus if you can find one.
- interaction with interfaces: are generic BOUNDS in scope (`pair(T) where T is
  addressable`), or is that a later arc? Recommend, with the reason.
- interaction with the existing wrapper set. The parked question was whether one
  generic surface could make every optional, undefinable and wrapper constructor
  lower through the SAME companion-table scheme instead of a bespoke desugar
  each. Starting points named by the user:
  `v6/prolog/0_type_plane.pl:145-151` (wrapper inventory),
  `0_generic_expand.pl:125-176` (collection artifacts),
  `0_option_expand.pl:39-49` (the scalar versus reference split).
  **This is the actual prize.** Answer it.

## Section E: the codegen picture, closed

The user's words: "its now open discussion to finish the codegen dreams". So
close the loop across everything pass 1 and pass 2 cover. One diagram and one
table showing, for a `.dl6` program using modules, visibility, enums,
interfaces and generics, what lands in TypeScript, Rust and Go. Name every
piece that is still missing after all forks are taken.

## Anti-cheat

| tempting shortcut | why it is a lie |
|---|---|
| new doc files | you append to pass 1's two files |
| re-deriving pass 1 | cite it |
| designing generics without reading `0_generic_expand.pl` | 348 lines of the answer are already written |
| asserting a probe result | rerun each one |
| picking a fork | you price, the user rules |
| leaving section D's wrapper question unanswered | it is named as the prize |

## File ownership
YOURS: `plans/2026-08-12-typespec-module-ir.RESEARCH.md` and its
`.visual.human.unga.md` twin. Everything else is READ ONLY.

## Style laws, inline
- No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source/origin, base layer, critical, mode.
- The word "refusal" is banned in prose; an error for an unbuilt construct is
  "TODO" or "not built yet".
- No sycophancy, no negative parallelism ("not X, Y" / "this isn't X. it's Y").
- Construct names use ONLY rxjs, prolog, or SQL words.
- dl variable names are descriptive, never single-letter, in every snippet.
- Surrogate keys: INTEGER ids, natural keys once in a dictionary table. Read
  `.claude/skills/sql-relational-design` before pricing any storage shape.
- Docs open with a table of contents.

## Worktree setup, before your first commit
The pre-commit hook needs the extractor binary and two pnpm installs:
```
(cd v6/sprefa-extract && cargo build --release --features cli --bin extract)
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
