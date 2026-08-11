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

## Where pokeapi stands

You asked for pokeapi done. It is not done, and the reason is NOT the two
things above. Those work.

```
  12 columns still get flattened.
  All 12 of them, every single one, hit the same name clash:

     the converter names a lifted sub-shape        item__kid
     the compiler names the optional-link table    item__kid

     same name, two different things, program stops.
```

I proved it: rename the converter's sub-shape, and 12 becomes 4. I did not keep
that change, because the converter is not this lane's file and renaming touches
every generated shape name in a checked-in file. That is your call, or the
coordinator's.

The last 4 are a genuine question only you can answer:

```
  rel contest_combos_normal(use_before: optional list, use_after: optional list).
```

Both columns are optional links, so BOTH move out to side tables, and the shape
is left with zero columns. Then something else points at it. A row with no
columns is indistinguishable from every other row with no columns, so there is
nothing to point AT.

Three ways out, none of them cheap enough for me to pick:

1. leave it stopping, but with a message that says what is actually wrong
2. let a column-less shape be identified by its hidden row number
   (breaks the rule that identity is either your key or your whole row)
3. have the converter not create column-less shapes in the first place

## Also found

- Writing `optional set-of-shapes` used to sneak past a ban that the plain
  spelling already enforced. It now stops the same way. That is a stop that
  MOVED, and it is the only one.
- A shape that is both pointed-at AND has a declared key loses its key when the
  compiler prints it back out as text. Separate bug, separate file, worked
  around here by not using a key in that one test.

## Receipts

```
  compiler tests        380 pass, 0 fail   (was 372)
  print/reparse         380 / 380
  text-vs-term door     276 / 276 byte identical
  two-parser agreement  699 / 699, 0 differences
  unit tests            604, 1 red, and that one is red before I touched anything
  new unit tests        6, all 6 fail on the starting commit
```
