# dot access / namespacing (dl6)

what the owner reads. plain words. no receipts.

---

## the ask

you want dotted access to be real:

    spine.files
    row.column

it got circled many times, never ruled. this plan recons everything we said
before, then costs four ways to make it real. no pick yet. you answer one
sentence each at the end.

---

## what we already said (short)

- dots were "nested pattern sugar", kept in the spec, never built past a note.
- one note said dot = a JOIN (follow a link to another table).
- another note said dot = a PROJECTION (pick a column).
- those two never agreed. that disagreement is the whole fight.
- rel names today: one flat wall, no namespaces. `spine_files` not `spine.files`.
- real namespacing already lives off-screen, in the scip symbol id
  (a `pkg` field keeps catalogs apart). so naming is done, just not on this
  surface.

left open from before: the "key vs arrow" thing. what sits left of `->` is the
demand key. dot on that would name its columns. still open.

---

## the surface today

an identifier = letters, numbers, underscore only. no dot. small check:

    ident:  a-z A-Z 0-9 _
            (must start letter or _)

a dot is already used for three things:

    1. end of every statement   (the period)
    2. inside a float           (1.5)
    3. nothing else

so if a dot means "member access", the parser must tell the difference between
a member dot and a sentence-ending dot. trick: member dot is glued to the next
word (`row.column`), ending dot is followed by a space or end. workable, but
`x . y` with spaces gets fuzzy.

the printer must print dots back the same way, or round-trip breaks. today
round-trip is 100%.

the old grammar file (langium) is demoted to a spelling reference. not a gate.
so changing the surface only touches the main parser + printer + one doc. cheap.

corpus: 360 dl6 files, ~376 rel decls. a rename sweep if we pick dots. mechanical.

---

## the typespec trap (the one you named)

typespec has this oddity, and it is exactly what you do not want:

    ONE dot, THREE jobs
    1. declare nested name:   namespace A.B.C
    2. reach a type:          SampleNamespace.SampleModel
    3. reach a property:      value.property

    plus TWO container kinds share the dot:
    a NAMESPACE (holds types)
    a MODEL     (holds properties)

you cannot tell what `A.B` means from the spelling. you must know whether A is
a namespace or a model, and whether you are in a type spot or a value spot.
a model and a namespace with the same name cannot both exist.

the sentence: "meaning depends on the KIND of the left side, guessed at the
use site." that is the failure mode. the plan's whole point is to dodge it.

test for any design:

    if the meaning of a name is obvious from the spelling + declared kinds
    -> good
    if the reader has to guess the left side's kind     -> bad (typespec)

---

## four ways to make dots real

### A. one dot, two jobs, guessing

    spine.files  could be:
      namespace spine, member files
      rel spine, column files
      type spine, field files

the parser must infer. when both readings are live, it refuses:
`ambiguous_dot(spine.files)`. honest, but the reader still can't tell. this
one keeps the typespec trap.

    ambiguous example:
      rel row(file, column)                    <- member reading
      namespace row { type column }            <- name reading
      write: row.column
      -> refusal. identical spelling, two meanings.

### B. two symbols: `::` for names, `.` for members

    spine::files   always a NAME (namespace member)
    row.column     always a MEMBER (pick a column of a typed row)

zero guessing. spelling decides. this directly kills the typespec trap.

    row.column = pick column "column" out of a typed row
    spine::files = go into the spine namespace, grab files

`::` has no collision (not a period, not a float, not an operator). only `.`
still needs the ending-dot trick. reads fine next to prolog's own `:` module
pun.

### C. no dots at all (today's answer)

    spine_files
    SCIP id does the namespacing off-screen

zero cost, zero migration. you lose:
    - pretty names (spine_files not spine.files)
    - no row.column projection at all; get a column by naming it at the call
      site, or by a decode join

### D. `::` only now, `.` member access later

namespacing is already done off-screen by the scip id. so maybe you want no
new name symbol, and the real missing piece is just row.column member access.
D says: if you want named groups on the surface, add `::`; treat `.` member
access as its own follow-on.

---

## cheat sheet

    design   parser  collision   guessing?        typespec trap
    A        25-45   period/flt  yes, can refuse  REPRODUCES
    B        30-50   period      no               dodges
    C        0       none        n/a              dodges
    D        15-25   none(::)    no               dodges

every one keeps the counting law: statements per tick = f(rules), never per
row. a dot that loops row by row is a defect; project or join flat, never
loop.

every one has a clean rxjs lowering:
    pick a column  -> already a column in the stream
    follow a link  -> the decode join, already built
    split a name   -> happens before lowering, no stream effect

---

## interfaces / traits?

the places we already fake a contract:
    host executor contracts  (a list of columns per host)
    TS "I" interfaces        (host's own typing, never in the language)
    the old "one rel one rule kind" law (already dead)

verdict: NO. first-class traits do not pay for themselves here. the only real
contracts are row shapes = lists of columns. we already have "a named list of
columns" (the type/struct). a trait would just wrap that in a name and add a
type-with-members feature whose one consumer is the same list. structural
typing has no subtyping in the datalog core, and no rule asks for it.

smallest thing that would cover the cases = a named column-list type, which
already exists. add a trait the day a rule needs to constrain a rel by an
abstract contract. none does.

---

## asks to you (one sentence each)

1. dot at all, or is call-site naming + scip id the ceiling (design C)?
2. if dot: is spine.files a MEMBER of a spine value, or a NAME group
   (namespace)? the two spell the same today, and that sameness is the trap.
3. if we keep one dot for both (A), ok with an "ambiguous_dot" refusal?
4. do you need `::` on the surface, or is the scip pkg id enough namespacing?
5. do you need traits for a reason beyond the host column lists, or are the
   fakes fine?
