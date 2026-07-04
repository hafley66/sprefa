# 8. Argmax and friends

> max vs argmax, the candidates/beaten/winner negation shape, per-key vs per-row grouping, the `key(...) merge(MaxBy(...))` lattice shortcut, and the SQL it lowers to.

**The question:** "what is the best row" comes up constantly (the current
value, the newest observation, the winning route). Datalog has no `ORDER BY
... LIMIT 1`. How do you express "the row that wins" as facts and rules?

This chapter reaches for a fresh fixture. Call graphs don't have a natural
notion of "best"; a small log of session bookmarks does.

```
mark("s1", 10, "draft")
mark("s1", 20, "v2")
mark("s1", 35, "v3-final")
mark("s2", 12, "notes")
mark("s2", 40, "notes-revised")
```

`mark(session, ts, title)`: every time a session's bookmark changes, a new row
is appended. Nothing is ever updated in place. The question "which title is
current?" is really "for each session, which row has the largest `ts`?"

## Max is a value; argmax is a row

`max(ts)` for session `"s1"` is `35`. That is a number, on its own it tells
you nothing about which title it belongs to. `argmax(ts)` for `"s1"` is the
whole row `("s1", 35, "v3-final")`. In SQL terms: `MAX()` is an aggregate
function that collapses a column to one value; argmax has to carry the rest
of the row along with the winning value. Datalog has no argmax keyword, so
you build it out of the same two tools every other rule uses: joins and
negation.

## The negation shape: candidates, beaten, winner

The standard idiom is three rules with one job each.

```
rel candidate(session: text, ts: int, title: text).
candidate(s, t, ti) <- mark(s, t, ti).

rel beaten(session: text, ts: int).
beaten(s, t) <- candidate(s, t, _), candidate(s, t2, _), t2 > t.

rel current_mark(session: text, ts: int, title: text).
current_mark(s, t, ti) <- candidate(s, t, ti), !beaten(s, t).
```

Read it as a sentence: a candidate is beaten when some other candidate in the
same session has a strictly greater `ts`. The winner is a candidate that
nobody beat. `beaten` is the negation's whole job, isolated in its own
relation so `!beaten(s, t)` reads as "no strictly-later mark exists for this
session."

This is safe under stratification (Chapter 2's requirement for negation)
because `beaten` depends only on `candidate`, and `current_mark` negates
`beaten`. There is no cycle: `beaten` is fully computed, every row settled,
before `current_mark` ever reads it with `!`. Compare this to `p :- not p`,
which has no fixpoint at all. Here the dependency graph is a straight line,
`candidate -> beaten -> current_mark`, so the layering is trivially there.

Run it on the fixture: for `"s1"`, `(10,"draft")` and `(20,"v2")` are each
beaten by `(35,"v3-final")`; `(35,"v3-final")` is beaten by nothing, so it
wins. For `"s2"`, `(40,"notes-revised")` wins over `(12,"notes")`.

## Per-key vs per-row: same shape, different join

`current_mark` groups by session: one winner per session, decided over the
whole history. A different question groups by something else entirely.
Suppose every session emits messages, and each message should see whichever
mark was current *when it was sent*, not the mark that is current *now*:

```
rel message(msg: text, session: text, at: int).
message("m1", "s1", 15).   -- sent between "draft" and "v2"
message("m2", "s1", 30).   -- sent between "v2" and "v3-final"
```

`m1` should see `"draft"` (the mark at ts=10, the latest one at or before 15).
`m2` should see `"v2"` (ts=20, the latest one at or before 30). Two messages,
same session, two different governing marks. The group key here is the
message, not the session, and the candidate set for each message is bounded
by its own `at`:

```
rel governs(msg: text, session: text, ts: int, title: text).
governs(m, s, t, ti) <- message(m, s, at), mark(s, t, ti), t <= at.

rel gov_beaten(msg: text, ts: int).
gov_beaten(m, t) <- governs(m, _, t, _), governs(m, _, t2, _), t2 > t.

rel governing_mark(msg: text, session: text, ts: int, title: text).
governing_mark(m, s, t, ti) <- governs(m, s, t, ti), !gov_beaten(m, t).
```

Same three-rule skeleton (candidates, beaten, winner), same stratification
argument. The only change is what feeds `candidate`: `governs` joins `mark`
against `message` first, so each message gets its own bounded slice of
history to pick a winner from. `current_mark` is the special case where every
row's `at` is "now" and the group key collapses to session. `governing_mark`
is the general case: per-row argmax, a different winner for every consumer.

## The engine shortcut: a lattice column

When the only question you will ever ask is "what's the current winner per
key" (never "what did row R see historically"), a lattice column declaration
does the same job at declare time, without writing `beaten` at all:

```
rel current_mark(session: text, ts: int, title: text) key(session) merge(MaxBy(ts)).
current_mark(s, t, ti) <- mark(s, t, ti).
```

`key(session)` says rows upsert per session. `merge(MaxBy(ts))` says when two
rows share a key, keep the whole row with the larger `ts`. This is per-key
argmax pushed into the storage layer: the engine keeps exactly one row per
key, always the current winner, and every insert is an upsert rather than an
append. `examples/mcp-echo.dl` uses exactly this for request dispatch:

```
rel route(id: text, result: text, prio: int) key(id) merge(MaxBy(prio)).
route(id, "pong", 100) <- req(id, "ping", _).
route(id, params, 100) <- req(id, "echo", params).
route(id, "unknown method", 1) <- req(id, method, _), !known(method).
```

Three rules compete to write `route` for the same request id; the lattice
keeps whichever has the higher `prio`, whole row and all, no `beaten` rule in
sight. `examples/net-atlas.dl` uses the same declaration for longest-prefix
route selection (`key(vrf, prefix) merge(MaxBy(prefix_len))`).

`examples/gh-cache.dl` and `examples/gh-cache-full.dl` want the same
per-key-winner idea (the freshest HTTP response per endpoint, "latest-wins")
but spell it a third way, by hand, with an aggregate head and a join back:

```
rel resp_latest(ep: text, b: int).
resp_latest(ep, max(b)) <- resp(ep, b, 200, _, _).
rel resp_current(ep: text, tag: text, body: text).
resp_current(ep, tag, body) <- resp(ep, b, 200, tag, body), resp_latest(ep, b).
```

`resp_latest` computes the winning bucket number per endpoint with `max(b)`
in the head. `resp_current` joins back on `(ep, b)` to fetch the rest of the
row. It is the same result as a `key(ep) merge(MaxBy(b))` decl on `resp`
would give, two rules instead of one, written before the lattice sugar
existed and left as is since it still works.

So three spellings for the same idea, pick by what you need:

| Form | Group key | Answers "what won historically for row R"? | Rule count |
|---|---|---|---|
| `candidate`/`beaten`/`winner` | anything you join on | yes, per consuming row | 3 |
| `max(col)` head, then join back | fixed at declare time | no, current winner only | 2 |
| `key(...) merge(MaxBy(...))` | fixed at declare time | no, current winner only | 1 |

Use the negation form when the argmax is per-row over a candidate set that
differs for every output row, like `governing_mark`. Use the lattice decl (or
its two-rule aggregate equivalent) when you only ever need the single current
winner per key going forward; it is fewer rules and the engine keeps one row
per key instead of the whole history.

## What it lowers to in SQL

The `winner <- candidate, !beaten` shape has a direct SQL translation, an
anti-join:

```sql
SELECT m1.session, m1.ts, m1.title
FROM mark m1
WHERE NOT EXISTS (
  SELECT 1 FROM mark m2
  WHERE m2.session = m1.session AND m2.ts > m1.ts
);
```

A second standard spelling uses a window function to number rows within each
group and keeps rank 1:

```sql
SELECT session, ts, title FROM (
  SELECT session, ts, title,
         ROW_NUMBER() OVER (PARTITION BY session ORDER BY ts DESC) AS rn
  FROM mark
) ranked
WHERE rn = 1;
```

A third spelling is SQLite-specific: put a plain, non-aggregated column next
to a `MAX()` in the same `SELECT ... GROUP BY`.

```sql
SELECT session, title, MAX(ts) AS ts
FROM mark
GROUP BY session;
```

SQLite documents that when a query has exactly one bare `min()`/`max()`
aggregate, every other bare column in that `SELECT` is taken from the same
row that produced the min/max, so `title` really is the winning title.
Most other databases (Postgres, MySQL in strict mode) reject this: a column
that is neither aggregated nor in `GROUP BY` is an error there, so this
spelling is not portable.

Which of these the engine actually emits, checked, not guessed: negation
lowers to exactly the first spelling. `!atom` in a rule body compiles to
`NOT EXISTS (SELECT 1 FROM <rel> <alias> WHERE <cond>)` (`src/lower.rs:153`),
so `winner <- candidate, !beaten` is that anti-join, generated. An aggregate
head like `resp_latest(ep, max(b)) <- resp(...)` lowers to an ordinary
`GROUP BY`: grouped head terms become the `GROUP BY` list, the aggregate term
becomes `MAX(b)` in the `SELECT` list (`src/lower.rs:295-334`). But the
generated query only ever selects the grouped columns and the aggregate
itself, never a bare extra column, so it does not lean on the SQLite-only
trick; fetching the rest of the row (`resp_current`) is a separate, ordinary
equi-join rule, portable SQL. `ROW_NUMBER()`/`PARTITION BY` do not appear
anywhere in the codebase; that spelling is presented here as the standard
alternative, not as something this engine generates.

## Intuition

> `max` gives you a value; argmax gives you the row. Build argmax from a join
> plus negation: candidates, a `beaten` relation that says a strictly better
> candidate exists, and a winner that is a candidate nobody beat. The group
> key lives in the join that produces `candidate`, so the same shape answers
> both "current winner per key" and "winner per row, over a bounded history."
> When you only need the former, a `key(...) merge(MaxBy(...))` lattice decl
> gets you there in one rule instead of three.

## Exercises

1. Write the three-rule shape for "the earliest mark per session" (smallest
   `ts`, not largest). What is the only line that changes from
   `current_mark`?
2. In `governing_mark`, what happens if two marks in the same session share
   the exact same `ts`? Does a message end up with one governing mark or
   two? Is that the same tie behavior as `current_mark`?
3. Rewrite `current_mark` as a `key(session) merge(MaxBy(ts))` decl (as
   shown above), then explain why that decl form cannot answer "what was
   governing when message `m1` arrived," only `governing_mark` can.
4. Given the fixture rows above, what does `winner` (Section 2, over all of
   `mark`) output for `"s2"`? What would a `key(session) merge(MaxBy(ts))`
   decl on the same rows keep?

## In your engine

`examples/mcp-echo.dl`'s `route` relation is the lattice form of this
chapter's idea, `key(id) merge(MaxBy(prio))`, deciding which handler rule
wins per request id. `examples/net-atlas.dl` uses the same decl shape for
longest-prefix route selection. `examples/gh-cache.dl` and
`examples/gh-cache-full.dl`'s `resp_latest`/`pr_latest_tx` relations are the
two-rule aggregate-then-join spelling of the same per-key winner, predating
the lattice decl and still in use. `src/lower.rs:153` is where `!atom`
becomes `NOT EXISTS`, the exact mechanism behind every `beaten`/`winner` pair
in this chapter. `src/lower.rs:295-334` is where an aggregate head like
`max(col)` becomes a `GROUP BY`.

## Answers

1. Flip the comparison in `beaten`: `beaten(s, t) <- candidate(s, t, _),
   candidate(s, t2, _), t2 < t.` Nothing else changes; `winner` still keeps
   whichever candidate nobody beats, "beats" now means "earlier."
2. Both tied marks satisfy `governs` for the same message with equal `ts`,
   and neither beats the other (`gov_beaten` only fires on a *strictly*
   greater `ts`), so both survive into `governing_mark` for that message.
   That is the same tie behavior as `current_mark`: a true tie in the
   ordering column produces two winners, not one. If a single winner is
   required on ties, add a tiebreaker column to the comparison (for example
   an insertion-order id) so `t2 > t` becomes `(t2, id2) > (t, id)`.
3. `rel current_mark(session: text, ts: int, title: text) key(session)
   merge(MaxBy(ts)). current_mark(s, t, ti) <- mark(s, t, ti).` The decl
   keeps one row per session, always the newest, so it has already thrown
   away every older mark by the time you ask about it. There is no way to
   ask it "what did this key look like at some earlier time" because the
   older rows are gone; the negation form keeps every candidate row around
   and re-derives the winner per bounded slice, which is exactly what
   answering per-message questions requires.
4. `winner` for `"s2"` outputs `("s2", 40, "notes-revised")`; `(12,"notes")`
   is beaten by it. A `key(session) merge(MaxBy(ts))` decl over the same
   rows keeps exactly the same single row per session, `("s2", 40,
   "notes-revised")` for `"s2"` and `("s1", 35, "v3-final")` for `"s1"`; for
   this fixture (no ties) the two forms agree, they differ only in what
   they retain and can still answer afterward.
