# Catalog backbone, in plain words

## The one sentence

The compiler already keeps a little table listing the rels you wrote.
It does NOT list the ~20 helper tables it builds behind each one.
This plan makes it list them too, so "which table belongs to what" is
something you can ask, instead of something you grep for.

## What the table knows today

```
   you write:          rel  file(path, size)
                              |
   catalog says:       [rel file]  [column path]  [column size]
                              |
   compiler ALSO built:  __delta_file   __frontier_file   __next_frontier_file
                         __txt_file     __pre_file        ... and more
                              ^
                         catalog says nothing about ANY of these
```

Those unnamed helper tables are where all five interning bugs lived.
Every one of them was "this column holds an id, that one holds letters,
and nothing wrote it down anywhere."

## The numbers

| thing | count today |
| --- | --- |
| compiled programs in the corpus | 220 |
| rows the catalog holds, all programs | 3,720 |
| helper tables the compiler builds, all programs | 4,458 |
| helper tables in a typical program | 13 |
| catalog rows in a typical program | 13 |

So naming the helpers roughly doubles the table. That is the whole cost.

## The three moves

| move | plain words | size |
| --- | --- | --- |
| 1. planes | every helper table gets a row, pointing at the rel it came from | biggest, ~200 lines |
| 2. storage | every column gets a row saying "id" or "letters" | small, ~65 lines |
| 3. ports | every shell command and timer gets a row | small, ~80 lines |

Then the audit stops being a special program and becomes a question:
"show me every column that stores ids but has no decoder."

## The trick that keeps it cheap

Row numbers are handed out in order and never change. So every new family
gets tacked on the END:

```
 [1..5] primitives    <- untouched
 [....] lists         <- untouched
 [.]    the module    <- untouched
 [....] rels+columns  <- untouched  ... and this is where the emitted
 ---------------------------------      TypeScript stops copying
 [....] planes        <- NEW
 [....] ports         <- NEW
 [....] storage       <- NEW
```

Because the new stuff is all at the end, and the emitted TypeScript only
copies the top part, **not one of the 220 compiled programs changes a single
byte** through the whole arc. No regenerating. No re-pinning baselines.
That is the difference between a two-day arc and a two-week one.

## The ladder

```
  step 0  delete a dead argument        2 lines     30 min
  step 1  give the table a real key    45 lines     half a day
  step 2  split the row producer       40 lines     half a day
  step 3  name the plane tables       200 lines     the real work
            |
            +--> step 4  the aggregate helpers    95 lines
            +--> step 5  shell commands + timers  80 lines
            +--> step 6  id-vs-letters per column 65 lines
                          |
  step 7  the audit itself, both doors           195 lines
```

Steps 4, 5, 6 do not touch each other and can run as three parallel lanes.
Everything before step 3 is one at a time.

## What is already broken and gets fixed on the way

| found | what it is |
| --- | --- |
| a dead argument | the compiler prints a warning every time it loads. Two lines fixes it. |
| the key is 11 columns wide | the table already has a perfectly good single number to key on and does not use it. In one of the two build modes, five of those eleven key columns are text, which the repo's own storage rules call a defect outright. |
| a stale test comment | says column types collapse to zero. They stopped doing that last week. |

## The biggest risk

Step 3 writes down "this program has a delta table for `file`."
If the compiler's real DDL and the catalog's claim ever disagree, the
catalog is now lying, and a lying catalog is worse than no catalog.

The defense is one test, and it is the point of the whole arc:

```
  for every compiled program:
      names of tables the program actually creates
                        ==
      names the catalog claims it creates
```

Not per-fixture. Across the whole corpus, as one family check. That is the
same shape as the rail that caught the fifth interning door, and it is what
stops a sixth.

## One thing that cannot be done yet

The reference engine (the thing that grades every fixture) does not have this
table at all. So a normal fixture that reads the catalog is graded WRONG
automatically, no matter how correct it is.

Two ways out:

| option | cost |
| --- | --- |
| teach the reference engine about the catalog | real work, and it puts a compiler-owned table into the grader |
| cover it with unit tests plus a live server rail instead | cheaper, leaves one coverage gap open |

This one needs your word.

## Also needs your word

1. Keep the new rows out of the emitted TypeScript? (yes = the whole arc is
   byte-free; no = 220 files regenerate at every step)
2. Call the decoder rows `view` or `decode_view`?
3. Steps 4, 5, 6 as three lanes at once, or one after another?
