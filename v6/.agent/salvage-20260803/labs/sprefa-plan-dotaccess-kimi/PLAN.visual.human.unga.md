# Dot access in dl6, explained plainly

Read this one. The other file (PLAN.md) is the receipts.

## The question

Can we write this?

    file.path
    span.file.name

instead of this?

    decode(File, {at: At}), decode(At, {name: Name})

Short answer: yes. Almost all the machinery already shipped.
What is missing is only the spelling.

## What the recon found

Dots have been circled five times:

1. v4 faked them. `X.field` was secretly the flat name `X_FIELD`.
   Three different fake implementations. Never real navigation.
2. A 2026-07-21 design doc designed the whole thing:
   a dot is a join through a lookup table.
   Storage stays flat. The dot is just sugar on top.
3. A later audit said: keep it, it is the cheapest construct in the spec.
4. A grader kernel registered `dot_access` as a sugar row.
   But the real parser never got a dot rule.
5. One ruling said dot paths are just join chains,
   and killed a fancier path language as redundant.

Meanwhile, the compiler grew exactly the tables the dot needs:

    rel span(start: int, end: int).
    rel mark(at: span).

A column typed by another rel is stored as a plain id.
There is a hidden dictionary table per type.
`decode` already reads fields through it, one cheap join per hop.

So the situation is:

    the 2026 design said:  dot lowers to a dictionary join
    the shipped compiler:  has the dictionary joins (decode uses them)
    missing:               the dot itself

## What a dot would touch

The parser: small. Identifiers are letters and underscores only.
The `.` is the statement end, but a dot chain starts after a name
inside an expression, so they never fight. Floats already prove the
boundary works: `1.5.` parses as a float, then the statement end.

The printer: one clause. Round-trip tests must stay green.

The meaning: one rewrite pass turns `x.f` into the same join
`decode(x, {f: ...})` already makes. Both compiler doors see the
rewrite, so the fast path and the checker path cannot drift apart.

The generated code: nothing new. SQL and JS for the join already exist.

## The typespec trap (the thing to avoid)

In typespec, `A.B` can mean several different things:

    namespace member      Foo.Bar          <- dot
    enum member           Direction.North  <- dot
    model property        Pet.name         <- NOT the dot
    model property meta   Pet.name::type   <- a second sigil

Which one you get depends on HOW A was declared,
not on what you are looking at.
The most natural reading (field of a record) is the one the dot skips.
That is the oddity. Two spellings, invisible dispatch, silent surprises.

## The three options

### Option A: one dot, fields only

`.` only ever follows a variable.
A variable always has a declared column type.
The type says which fields exist.
So `x.f` always means: read field f of the record x points at.

    value plane:    x.f      what x IS decides
    decl plane:     rel, sh  names never take a dot

The typespec trap cannot happen:
there is only one member space (fields of the receiver's type),
and the spelling never depends on a faraway declaration kind.
If x is text, `x.f` is a clean named error, not a surprise.

The ambiguous case and its answer:

    rel file(path: text).
    scan_out(file), P = file.path

Here `file` is a variable (lowercase name, so it binds).
It holds text. Text has no fields. Named refusal.
The other reading (column of rel file) is refused by one rule:
a bare name in an expression is always a variable.

Cost: about 25 lines parser + printer, one rewrite pass,
a few named refusals, count tests so long chains stay flat.
Migration: zero. `decode` stays legal forever.

### Option B: `::` for namespaces, `.` for fields

    spine::files(x)     namespace
    x.file              field

Zero ambiguity, by construction.
But nobody needs namespaces yet:
there is no `use`, no imports, one flat rel space,
and internal tables already hide behind a `__` prefix.
It also rebuilds the two-sigil world typespec has,
just with the sigils swapped.
Cost is real too: rel names become two-part keys
and the JS emitter needs a name mapping.

Verdict in the plan: not today. Revisit when imports land.

### Option C: no dots

Keep writing `decode(File, {at: At})`.
Costs nothing. Reads like the workaround it is:
one extra line and one temp variable per hop, six times
in one real fixture family. The nice reading
`call.loc.file.rev.repo` stays impossible.

## Do we need `::` symbols?

No. Checked every possible user:

- internal tables: already hidden behind `__`
- sharing between programs: does not exist yet (no imports)
- cross-language symbol names: live inside SCIP strings, which
  are data, not syntax

When imports arrive, option B's price sheet is already written down.

## Do we need interfaces or traits?

No. The working fakes are all "a contract written as data,
checked at compile time":

- host contracts are facts in a registry table
- a `sh` decl's column list is the contract for its raw template

The one fake that leaks is on the TypeScript side,
where header interfaces drift because nothing checks them.
The sibling type-ir lane is already building the checker for that
(the same facts-plus-staleness-gate trick).
The language's interface construct already exists: it is the `rel` decl.

## The one decision, then small ones

1. A, B, or C?  (The plan recommends nothing; it prices all three.
   A is the one that dodges the typespec trap and costs almost nothing.)
2. If A: should `rel.column` (dot straight on a rel name) exist now,
   or wait for imports?
3. If A: dot on plain json columns too, or only typed records first?
4. Rewrite the 72 existing decode calls to dots, or leave them?
5. Update the old grammar's spelling notes now or later?

## The shape of the work if you pick A

    ruling (you)
      |
      v
    parser + printer        dot chain parses, round-trip stays 136/136
      |
      v
    rewrite pass            x.f.g  ->  the joins decode already makes
      |
      +--> named refusals   dot on text, unknown field, unbound source
      |
      +--> count tests      chain of 5 joins must not cost 25 statements
      |
      +--> rx snippet       the reactive lowering, written in the header

    later, only if imports land:
    namespace_revisit       option B's price sheet is ready
