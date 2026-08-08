# lists get real, in plain words

## contents

- [the one sentence](#the-one-sentence)
- [what a list is here](#what-a-list-is-here)
- [what is broken today](#what-is-broken-today)
- [the fix, in two steps](#the-fix-in-two-steps)
- [what you can newly write](#what-you-can-newly-write)
- [what stays banned](#what-stays-banned)
- [who does what](#who-does-what)

## the one sentence

A list column currently forgets it is a list halfway through the compiler, so
the database never checks that it holds an array, and you cannot nest one.

## what a list is here

A list is ONE value living in ONE column, written as json text.

```
   repo table
   +----+---------+---------------------------+
   | id | name    | tags                      |
   +----+---------+---------------------------+
   |  1 | sprefa  | ["rust","prolog","sql"]   |   <- one cell, one value
   +----+---------+---------------------------+
```

That is different from a list of THINGS, which is a second table:

```
   file table              file_tag table
   +----+-------+          +---------+-----+
   | id | path  |          | file_id | tag |
   +----+-------+          +---------+-----+
   |  7 | a.rs  |          |    7    | rs  |
   +----+-------+          |    7    | lib |
                           +---------+-----+
```

The rule: a list holds VALUES. Anything that has its own identity gets a table.

## what is broken today

The type walks down the compiler and loses its name partway:

```
  you write         list(text)
       |
       v
  type checker      list(text)          still knows
       |
       v
  storage step      json                <-- FORGETS HERE
       |
       v
  table builder     json                 cannot ask for an array
       |
       v
  SQLite            "is this valid json?"   <- the only check that exists
```

Two consequences:

1. The database will happily store `{"a":1}` in your list column. It is valid
   json. It is not a list.
2. You cannot write a list of lists, because the checker refuses anything that
   is not one of four simple types.

## the fix, in two steps

**step 1 - stop forgetting.** Carry `list(text)` all the way down instead of
flattening it to `json`. Then the table builder can ask for one more thing:

```
  before   CHECK (json_valid(tags))
  after    CHECK (json_valid(tags) AND json_type(tags) = 'array')
```

Nothing you can write changes. The database just starts catching a mistake it
used to wave through.

**step 2 - widen what goes inside.** Today the allowed element types are:

```
  int   text   bool   float
```

After:

```
  int   text   bool   float   json   list(...anything in this row...)
```

SQLite cannot check the elements itself, so the compiler checks them when a row
arrives. Wrong element, named error, row refused.

## what you can newly write

```
  rel matrix(id: int, rows: list(list(int))).
  rel batch(id: int, payloads: list(json)).
```

Both are refused today.

## what stays banned

```
  rel doc(id: int, spans: list(span)).      <-- still refused
```

A `span` is a row with its own id. Putting ids inside a list would leak ids into
the tick log, and the tick log prints values, never ids. That rule is not being
touched by this work.

## who does what

```
  lane 1  listkind      the list work above
                        files: type plane, lowering, analysis, emitter

  lane 2  variantfield  unrelated bug: a field inside an enum variant that is
                        declared float or bool gets silently stored as int
                        files: enum expansion only

  chris's coordinator   rebasing the older catalog lane onto current main
```

Lane 1 and the coordinator both touch the lowering file, in parts far apart.
The catalog lane lands first.

Gate for everyone: `cd v6 && just green-all`, about three minutes, 31 checks,
exit 0 or it did not happen.
