# 5. Negation and argmax

> newest-per-group with the candidate / beaten / winner shape.

**Goal:** find the newest row per group, the thing datalog has no `ORDER BY ...
LIMIT 1` for, using the candidate / beaten / winner shape.

"The best row per group" comes up everywhere: the current version, the latest
observation, the winning route. Datalog answers it with a join and a negation,
not a sort. This lesson is a small taste; chapter 8 of the book is the full
treatment, including the `key(...) merge(...)` shortcut.

## The fixture is facts, not files

This lesson needs a natural notion of "newest," which a call graph does not have.
So it declares its own facts inline. A relation can be filled by literal rows,
no scan involved. Save as `05.dl`:

```dl
rel edit(note_id: text, version: int, title: text).
edit("n1", 1, "draft").
edit("n1", 2, "second pass").
edit("n1", 3, "final").
edit("n2", 1, "todo").
edit("n2", 2, "todo revised").

rel beaten(note_id: text, version: int).
beaten(note_id, version) <-
    edit(note_id, version, _),
    edit(note_id, other_version, _),
    other_version > version.

rel latest_edit(note_id: text, version: int, title: text).
latest_edit(note_id, version, title) <-
    edit(note_id, version, title),
    !beaten(note_id, version).

? latest_edit(note_id, version, title).
```

Each `edit(...)` line with only literals is a fact. `edit` records a note's title
at each version; nothing is updated in place, a new row is appended. The question
"what is each note's current title" is "for each note, which row has the highest
version."

Three relations, one job each:

- `beaten` says a version is beaten when the same note has some `other_version`
  that is strictly greater. This is where the join and the comparison live.
- `latest_edit` keeps a row only when it is *not* beaten. `!beaten(...)` is
  negation: the row must not exist in `beaten`. That is the anti-join.

Negation is safe here because the relations form a straight line,
`edit -> beaten -> latest_edit`. `beaten` is fully computed before
`latest_edit` reads it with `!`. Chapter 8 explains why that layering (called
stratification) is required.

## Run it

```sh
dl 05.dl --root notes-app --no-daemon
```

(The `--root` is ignored here since nothing scans, but keep the habit.)

## Expected output

```
? latest_edit => note_id	version	title
  n1	3	final
  n2	2	todo revised
  (2 rows)
```

For `n1`, versions 1 and 2 are each beaten by 3, so `("n1", 3, "final")` wins.
For `n2`, version 1 is beaten by 2, so `("n2", 2, "todo revised")` wins. Note the
shape carried the whole winning row along, `title` and all. A plain `max(version)`
would give you the number `3` with no idea which title it belonged to. Argmax is
a row, and you build it from a join plus a negation.

## Exercise

Change `beaten` to find the *earliest* edit per note instead of the latest. Only
one character changes. (Chapter 8, exercise 1, has the answer if you get stuck.)
Then read chapter 8's "engine shortcut" section and rewrite `latest_edit` as a
one-line `key(note_id) merge(MaxBy(version))` declaration.
