# Optional lists of shapes, in plain words

## What you asked for

Two things should be legal to write:

```
  a shape that MAY have a list of other shapes      option(list(X))
  a shape that MAY link to another shape            option(X)
```

Both are legal now. You also said: no cheap list-specific hack. There is none.

## The one idea

A column type is a stack of wrappers around a name.

```
  option( list( span ) )
     |      |      |
     |      |      +--- the name at the bottom
     |      +---------- a wrapper
     +----------------- a wrapper
```

Before, the code that dug down through that stack existed in FIVE places and
each one had its own hand-written list of which wrappers it knew about. None of
them knew about "option".

Now there is ONE digger, and ONE list of wrappers, and every place calls it.

```
  BEFORE                          AFTER

  parser door 1 --> own list      parser door 1 --\
  parser door 2 --> own list      parser door 2 ---\
  interned check -> no list       interned check ---+--> the one digger
  list-flavor check -> own list   list-flavor check-/
                                                   /
                                  one wrapper list-
```

## The one thing that is NOT the same for every wrapper

While doing this I found a real difference, and it matters.

```
  a LIST of shapes:
     the list gets its own little table, and one column in it
     holds the shape itself.
        -> the shape needs its cheat sheet

  an OPTIONAL link to a shape:
     the link gets its own little side table, and the column in it
     holds only a NUMBER, the shape's id.
        -> the shape does NOT need its cheat sheet
```

So the wrapper list is not just "here are the wrappers". Each wrapper also says
which of those two things it does. Five rows, two answers.

I found this the hard way: the first version treated every wrapper the same,
every test I had went green, and a DIFFERENT gate went red. A shape that only
ever appeared behind "optional" suddenly grew a whole extra storage plane it
had never had. Caught, fixed, and that gate is byte-for-byte identical again.

## The second bug, unrelated to the first

```
  you write:     shape "commit" has an OPTIONAL link to a person
  compiler does: removes that column from commit
                 puts it in a side table
  compiler forgets: the cheat sheet describing commit still lists it

  later: a checker reads the cheat sheet, sees a column that
         does not exist, and gives up.
```

The old code patched the cheat sheet one column at a time, and only when it
could still find that column. A column that was REMOVED could never be found,
so it was never patched.

The fix is not "add a case for removal". The cheat sheet's whole job is to say
what columns the shape has. So it is now just re-read, whole, after the
compiler finishes rearranging. Renames, removals, anything: one read.

## Where pokeapi stands: 12 flattened columns became 4

The 12 were never about the two things above. Every single one hit a name
clash:

```
  the converter names a lifted sub-shape        item__kid
  the compiler names the optional-link table    item__kid

  same name, two different things, program stops.
```

Fixed on the converter side: a sub-shape lifted out of an OPTIONAL property is
now called `item__kid_object`. Only optional ones get renamed, so 12 names
change instead of 161, and both options land on the same final count.

```
  BEFORE            AFTER
  12 flattened      4 flattened
```

The compiler also used to report that clash as "this name has two different
column counts", which tells you nothing about which feature did it. It now says
which shape, which column, and that the optional-link table is the thing that
claimed the name.

## The last 4, and the one question I need you to answer

```
  rel contest_combos_normal(use_before: optional list, use_after: optional list).
```

Both columns are optional, so BOTH move out to side tables, and the shape is
left with ZERO columns. Then something else points at it.

A row with no columns cannot be told apart from any other row with no columns.
So two different move entries would end up sharing one row, and one set of side
table entries. That is data loss, not a missing feature.

The compiler now stops on this with a message that says exactly that, instead
of the old confusing "this shape you declared is unknown".

**Your question: when a shape has zero columns left, what identifies one of its
rows?**

```
  1. nothing does. keep stopping.          <- what it does today
  2. its hidden internal row number.       <- reaches zero flattened columns,
                                              but breaks the rule that a row is
                                              identified by its key or its whole
                                              contents
  3. the converter gives up nullability    <- reaches zero flattened columns,
     on those 4 columns                       but the "did nullability survive"
                                              counter goes from 786/0 to 782/4
```

Only option 2 reaches "zero flattened columns" without moving a different
number. I did not pick.

## Also found

- Writing `optional set-of-shapes` used to sneak past a ban that the plain
  spelling already enforced. It now stops the same way.
- A shape that is both pointed-at AND has a declared key loses its key when the
  compiler prints it back out as text. Separate bug, separate file I do not own,
  worked around here by not using a key in that one test. Still open.
- Nine compiler stops belonging to the optional-value and generic-shape
  features were printing a bare "compiler refused rule X" with no shape named,
  because their two files were missing from the message system's source list.
  Added; they now name the shapes.

## Receipts

```
  compiler tests        382 pass, 0 fail   (was 372)
  print/reparse         382 / 382
  text-vs-term door     276 / 276 byte identical
  two-parser agreement  701 / 701, 0 differences
  unit tests            604, 1 red, and that one is red before I touched anything
  new unit tests        6, all 6 fail on the starting commit
  converter tests       9 pass, 0 fail     (was 7)
  pokeapi flattened     4                  (was 12)
```
