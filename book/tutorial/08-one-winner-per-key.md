# 8. One winner per key

> the `key(...) merge(MaxBy(...))` lattice declaration: lesson 5's argmax as one line, then rule priority as dispatch.

**Goal:** replace the candidate / beaten / winner shape with a one-line
declaration, and learn the dispatch pattern it unlocks.

Lesson 5 built "newest row per note" out of three relations. That shape is worth
knowing because it composes with anything. But when all you want is "keep one
winning row per key," the engine has a declaration for it. Chapter 8 of the book
compares the two forms in depth; here you run the short one.

## The program

Same five `edit` facts as lesson 5. Save as `08.dl`:

```dl
rel edit(note_id: text, version: int, title: text).
edit("n1", 1, "draft").
edit("n1", 2, "second pass").
edit("n1", 3, "final").
edit("n2", 1, "todo").
edit("n2", 2, "todo revised").

rel latest_edit(note_id: text, version: int, title: text) key(note_id) merge(MaxBy(version)).
latest_edit(note_id, version, title) <- edit(note_id, version, title).

? latest_edit(note_id, version, title).
```

The two qualifiers on the declaration do all the work `beaten` did:

- `key(note_id)` says rows agree on identity when they agree on `note_id`.
- `merge(MaxBy(version))` says when two rows share a key, the one with the
  greater `version` survives.

The rule itself is now a plain copy. Every `edit` row is offered; the lattice
keeps one per key.

## Run it

```sh
dl 08.dl --no-daemon
```

## Expected output

```
? latest_edit => note_id	version	title
  n1	3	final
  n2	2	todo revised
  (2 rows)
```

Byte-identical to lesson 5's output, from one relation instead of three.

## Priority dispatch

The merge function does not care which *rule* produced a row, only which row
wins. That makes a priority column a router: several rules propose answers for
the same key, and `MaxBy` keeps the strongest. Save as `08-dispatch.dl`:

```dl
rel request(id: text, method: text).
request("r1", "ping").
request("r2", "echo").
request("r3", "frobnicate").

rel route(id: text, result: text, prio: int) key(id) merge(MaxBy(prio)).
route(id, "pong", 100)      <- request(id, "ping").
route(id, "echoed", 100)    <- request(id, "echo").
route(id, "unhandled", 1)   <- request(id, _).

? route(id, result, prio).
```

The third rule fires for *every* request, so every id is guaranteed a row. For
`r1` and `r2` a prio-100 rule also fires, and `MaxBy(prio)` discards the
fallback. Only `r3`, which no specific rule claims, keeps it:

```
? route => id	result	prio
  r1	pong	100
  r2	echoed	100
  r3	unhandled	1
  (3 rows)
```

Note what did not happen: no negation. Lesson 5 would have needed a
`!handled(id)` guard on the fallback rule. The lattice replaces "suppress the
loser" with "offer everything, keep the winner." Lesson 13 builds a working
JSON-RPC server on exactly this relation.

## When to still use the lesson 5 shape

`merge` keeps only the current winner. If a later tick retracts the winning row,
the runner-up correctly resurfaces, but you can never *query the losers*: the
beaten rows are gone. When downstream rules need every candidate (an audit
trail, a "second best," a tie report), build the negation shape. When you need
one answer per key, declare it.

## Exercise

Change one word in `08.dl` so `latest_edit` becomes `earliest_edit`, keeping
the oldest title per note. Then check your understanding against `08-dispatch.dl`:
what happens if two rules propose rows for the same id with the *same* prio?
Run it (give `"r1"` a second prio-100 rule) and see which row survives.
