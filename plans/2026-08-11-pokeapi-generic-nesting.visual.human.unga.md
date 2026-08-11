# The pokeapi nesting gap, in plain words

## What this is about

We turn a big OpenAPI file (pokeapi, 212 shapes) into our own language, then
compile it. Some columns do not survive the trip. They get flattened into a
generic "json" blob instead of keeping their real type. We wanted to know why.

## The one-line answer

The old story was wrong. Almost everything works. Two small shapes stop the
compiler, and one of them is a one-line oversight.

## What we thought vs what is true

```
THE OLD STORY
  "If a shape is used inside another shape,
   it may not have any optional or list columns."
  -> 29 columns flattened

WHAT WE MEASURED
  Optional column inside a nested shape?      works
  List column inside a nested shape?          works
  Optional json column inside a nested shape? works
  List of shapes inside a nested shape?       works
  ...eight of these, all fine.

  Only two shapes actually stop:
    A. an OPTIONAL LINK to another shape
    B. an OPTIONAL LIST of another shape
```

## The two that stop, and why

### A. optional link

```
  shape "span" has an optional link to shape "lang"

  what the compiler does to it:
     span(start, spoken_in)          <- what you wrote
        becomes
     span(start)                     <- the link column is REMOVED
     span_spoken_in(span, lang)      <- and moved to its own side table

  what it forgets to do:
     the "cheat sheet" describing span for nesting purposes
     still lists the removed column. Nobody updates it.
     Later a checker reads the cheat sheet, sees a column
     that no longer exists, and gives up.
```

This is not a deep problem. It is a bookkeeping step nobody wrote. But the
right fix depends on a question only you can answer: when span is nested inside
something else, does it still show its optional link, or not? That changes what
the outside world sees, so it is your call.

### B. optional list of shapes

```
  a plain LIST of shapes           -> works today
  an OPTIONAL list of shapes       -> stops
```

The only difference is the word "optional" wrapped around it. The code that
walks a type looking for shape names knows how to look inside "list", and does
not know how to look inside "optional". So the shape inside never gets its
cheat sheet made, and the same checker gives up.

We tested the fix: teach that walk to look inside "optional" too. One line.
The optional list of shapes then compiles all the way through, and nothing else
changes. We did not keep the fix, because making a new thing legal in the
language is your decision, not ours.

There is a third shape, "optional list of raw json". That one is genuinely
unfinished, and it needs a naming decision before it can work. It does not
appear anywhere in pokeapi.

## The mystery number

A previous attempt at this reported the problem got WORSE: 75 flattened columns
instead of 29. That looked like the fix backfired.

It did not. Here is the machinery:

```
  The converter is careful. Before flattening anything,
  it actually TRIES to compile each suspicious shape,
  and only flattens the ones that really fail.

  If that try-it-out step cannot run at all,
  the converter plays it safe and flattens EVERYTHING suspicious.

  Everything suspicious = 75 columns. Exactly 75.
```

So the 75 was the safety net, not the fix. The previous attempt's own edits
probably broke the try-it-out step, and the converter fell back to flattening
all of it.

## What we changed

The converter used to work shape-by-shape: if a shape failed, every optional and
every list column on it got flattened, guilty or not. On the worst shape that
meant 24 columns flattened when only 7 were actually a problem.

Now it works column-by-column. It tries each column on its own and flattens only
the ones that really stop the compiler.

```
  BEFORE                            AFTER
  29 columns flattened              12 columns flattened
     8 real problems                   8 real problems
     4 real problems                   4 real problems
    17 innocent bystanders             0 innocent bystanders
```

Those 17 columns now keep their real meaning. "This number is optional" instead
of "this is some json, good luck".

We changed nothing in the compiler itself. The remaining 12 are the two shapes
above, waiting on your decision.

## Also found

One test in the repo was already failing before we started, and it was failing
because it asserted the old wrong story. It expected the compiler to reject a
program that the compiler happily accepts. We rewrote it to match reality and
gave it a real compile check so it cannot drift again.

Two other test legs were already red on the same starting point, unrelated to
any of this. We left them alone and reported them.

## What we need from you

Three questions, in order of how much they matter:

1. When a shape is nested inside another, and it has an optional link, should
   the nested view still show that link? (This unblocks 8 columns.)
2. Should "optional list of shapes" be a legal thing to write? It costs one
   line and stores exactly like a plain list of shapes, which already works.
   (This unblocks the other 4.)
3. Should "optional list of raw json" exist at all, given that an empty list is
   already a perfectly good value and is not the same as missing? (This unblocks
   nothing in pokeapi; it is a language tidiness question.)
