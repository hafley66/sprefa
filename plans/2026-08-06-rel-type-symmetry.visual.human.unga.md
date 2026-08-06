# rel = type, caveman version

Picture: `plans/2026-08-06-rel-type-symmetry.png`

## contents

1. [the one idea](#1-the-one-idea)
2. [two machines, same gears](#2-two-machines-same-gears)
3. [four hashes, four jobs](#3-four-hashes-four-jobs)
4. [the trap](#4-the-trap)
5. [file changes, five verdicts](#5-file-changes-five-verdicts)
6. [how far does red go](#6-how-far-does-red-go)
7. [what everyone else does](#7-what-everyone-else-does)
8. [build order](#8-build-order)

## 1. the one idea

You already built the hard half.

```
        TYPE system                       REL system
   "same values = same row"          "same name = same rel"

     UNIQUE (cols)                   UNIQUE (parent, name, arity)
          |                                     |
          v                                     v
     __id = rowid                          rel_id = rowid
```

Same trick. SQLite refuses the duplicate, SQLite hands back a number. You did
not write an intern table. You asked SQLite for one, twice.

## 2. two machines, same gears

```
   TYPE                            REL
   ----                            ---
   rel file(repo, at)              rel orchard { tree, fruit }
     |                               |
     +-- column repo                 +-- child tree
     +-- column at                   +-- child fruit
          |                               |
     order in CREATE TABLE           parent_id points up
          |                               |
     body_tag says which variant    kind says which sort of thing
          |                               |
     body + page -> body_page       orchard + tree -> orchard__tree
          |                               |
     collision? THROW               collision? THROW (not built yet)
```

One row differs, and it is the last one. The enum case cannot collide, because
one file cannot declare `page` twice. Two FILES can both declare `tree`. So the
rel system needs one extra ingredient: a number per file.

## 3. four hashes, four jobs

One hash cannot do this. Four can, and each one is allowed to break exactly one
thing.

```
h_id      = H(file, name, arity)        -> the NAME.  never moves.
h_schema  = H(columns, types, key)      -> changed? DROP + CREATE the table.
h_rule    = H(the rule bodies)          -> changed? keep table, redo rows.
h_rows    = H(the rows)                 -> changed? wake whoever reads me.
```

Read it as a ladder. Each rung breaks more than the one above it.

```
   nothing broke        h_id same, h_schema same, h_rule same, h_rows same
   rows changed         h_rows differs                -> wake readers
   rules changed        h_rule differs                -> redo rows
   shape changed        h_schema differs              -> new table, rows GONE
   name changed         h_id differs                  -> a different rel entirely
```

## 4. the trap

You asked for the hash to be in the name. Watch what happens.

```
   rel orchard.tree(tree_id, species)
        -> table  orchard__tree__f9fc8ea9
                                 ^^^^^^^^ hash of the columns

   you add one column: picked

   rel orchard.tree(tree_id, species, picked)
        -> table  orchard__tree__3b1c02aa
                                 ^^^^^^^^ different hash

   SQLite now has TWO tables:

     orchard__tree__f9fc8ea9   <- all your rows, unreachable forever
     orchard__tree__3b1c02aa   <- empty
```

Every column edit becomes a rename. A rename in SQLite is a new empty table.
The old rows just sit there.

So: hash goes in a COLUMN, never in the name.

```
   name   = orchard__tree             (stable, boring, forever)
   column = h_schema = 3b1c02aa       (changes freely, tells you to rebuild)
```

Rust does the same thing. It hashes the CRATE and never the function body. If it
hashed the body, every edit would rename every symbol and nothing would ever
link.

## 5. file changes, five verdicts

You save file B. Compile file B alone. For each rel it declares, look it up by
`h_id`.

```
                        look up by h_id
                               |
        +----------+-----------+-----------+-----------+
        |          |           |           |           |
      MISS     h_schema     h_rule      all four     was there
               differs      differs      equal       before, gone now
        |          |           |           |           |
       NEW     RESHAPED    REBODIED      GREEN        GONE
        |          |           |           |           |
    CREATE     DROP +      keep table   do nothing   DROP
    seed       CREATE      DELETE +     at all       table
    rows       rows LOST   recompute
        |          |           |           |           |
     readers    readers    check         nobody      readers
     go RED     go RED     h_rows        wakes       go RED
```

GREEN is the one that matters. Save a file, change nothing real, and the server
does zero work. No DDL, no rows, no wake.

## 6. how far does red go

Red does not go all the way down. It stops the moment recomputing gives the same
answer.

```
   orchard__tree     RESHAPED           <- you edited this
        |
        | hop 1: recompute. rows differ. RED continues.
        v
   orchard__fruit    RED
        |
        | hop 2: recompute. rows differ. RED continues.
        v
   ripe              RED, recompute
        |
        | hop 3: recomputed ripe, and h_rows came out IDENTICAL.
        |         nothing actually changed. STOP.
        X
   report            never runs
```

Cycles are the one exception. If a red rel sits on a loop, the whole loop goes
red at once. Half a loop cannot be green.

## 7. what everyone else does

Nobody carries a flat string as the identity. Everybody carries a PAIR inside,
and flattens only at the exit.

```
   inside the compiler          at the exit
   ------------------          -----------
   Rust    (crate, item)  ->   _RNvCs7qp..7mycrate7example
   Go      (pkg, name)    ->   github.com/x/y.Foo
   SQLite  (schema, tbl)  ->   other.t
   python  "a.b.c" + attr ->   "a.b.c"
   you     (parent, name) ->   orchard__tree
```

Who hashes:

```
   Rust     hashes the CRATE.  never the item body.
   Go       escapes bad bytes. never hashes at all.
   Python   no hash. dotted string is the key.
   SQLite   no hash. two names, two slots.
   dl v5    no hash, no namespace. last writer silently wins.
```

Two of the five refuse on collision. None of the five hash content into a name.

Go's own comment says it plainly: a symbol is "an object name in a segmented
(pkg, name) namespace." Segmented. Two parts. Never joined until the linker.

## 8. build order

```
   h1  rename __catalog_rel -> __rel                    can do today
   h2  add arity to the name                            can do today, fixes a real bug
   h3  let a dotted name through the parser             the mangler has no input without it
   h4  module_id + h_id, one hash per FILE              two files stop colliding
   h5  h_schema + h_rule + the five verdicts            reshape stops being invisible
   h6  h_rows + red/green                               reloads stop being expensive
```

Two things worth knowing before you start.

**h2 is fixing something already broken.** `table_name` throws the arity away.
Declare `edge/2` and `edge/3` in one program and the compiler emits
`CREATE TABLE "edge"` twice. It has never fired because no fixture does it, but
it is sitting there.

**h5 is fixing something worse.** When you swap a program under a running
server, the swap re-runs the DDL and IGNORES every "already exists" error. So a
reshaped rel keeps its OLD table shape and nothing tells you. The rows go into a
table with the wrong columns, or the insert fails quietly.

## the three questions that actually need you

1. Which hash function? xxh3 is what the Go TypeScript compiler uses.
2. `__` for the module join, so `orchard__tree` never looks like the enum join
   `body_page`. Yes or pick another.
3. When a reshape would DELETE rows under a running server, should it just do it,
   or refuse and make you pass a flag?
