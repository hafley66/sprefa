# 3. Join and derive

> two source relations joined into a third derived one, and the one-relation-one-rule-kind law shown by triggering the engine's bail.

**Goal:** join two source relations into a third derived one, and meet the law
that a relation is written one way, not two.

Lesson 2 produced facts straight from files. Those are source facts. A **derived**
relation is computed from other relations, touching no file. The tool is the
join: two atoms that share a variable must agree on its value. Chapter 1 of the
book has the theory; here you run one.

## The program

You have two source relations already: functions defined, and functions called.
Join them to keep only the calls whose target is a function defined *in this
repo*. Save as `03.dl`:

```dl
rel fn_def(name: text, path: file, line: int).
fn_def(name, path, line) <-
    scan("src/**/*.rs", path, rev),
    ast(path, rev, :rust, "(function_item name: (identifier) @name)", line).

rel fn_call(callee: text, path: file, line: int).
fn_call(callee, path, line) <-
    scan("src/**/*.rs", path, rev),
    ast(path, rev, :rust, "(call_expression function: (identifier) @callee)", line).

rel resolved_call(callee: text, path: file, line: int).
resolved_call(callee, path, line) <-
    fn_call(callee, path, line),
    fn_def(callee, _, _).

? fn_call(callee, line).
? resolved_call(callee, line).
```

`fn_def` and `fn_call` are source rules (they read files). `resolved_call` is a
derived rule: its body mentions only relations. The join is the shared variable
`callee`. `fn_call(callee, ...)` and `fn_def(callee, _, _)` must agree on
`callee`, so a call survives only when some `fn_def` has the same name. The `_`
is "anything, do not bind it."

## Run it

```sh
dl 03.dl --no-daemon
```

## Expected output

```
? fn_call => callee	line
  drop	27
  log_note	23
  parse	13
  save	14
  (4 rows)

? resolved_call => callee	line
  log_note	23
  parse	13
  save	14
  (3 rows)
```

`drop` is gone. It is a call, but no `fn_def` names it (it comes from the
standard library, outside the repo), so the join drops it. The other three
targets are defined here, so they stay. That is a join doing real work: keeping
rows that agree, discarding rows that do not.

## One relation, one rule kind

There is a law worth learning by breaking it. A relation is written *either* by
source rules *or* by derived rules, never both. Watch what happens when you mix
them. Save as `03-bail.dl`:

```dl
rel thing(name: text, path: file, line: int).
thing(name, path, line) <-
    scan("src/**/*.rs", path, rev),
    ast(path, rev, :rust, "(function_item name: (identifier) @name)", line).
thing(name, path, line) <-
    thing(name, path, line), line > 100.

? thing(name, line).
```

`thing` is headed by a source rule (the `scan`/`ast`) and a derived rule (the
self-reference) at once.

```sh
dl 03-bail.dl --no-daemon
```

```
Error: relation 'thing' is written by both a source rule (scan/match/ast/...) and a derived rule; the scanned rows would be dropped on rebuild. Put the source rule and the derived rule in two separate relations and union them in a third derived rule.
```

The engine refuses, and the message tells you the fix. When it rebuilds derived
relations it clears them first, which would wipe the scanned rows. So it makes
you split: keep the source rule in one relation, the derived rule in another, and
union them in a third. That split is exactly what `03.dl` above does, three
relations, each written one way.

## Exercise

The message says to "union them in a third derived rule." Rewrite `03-bail.dl`
into two relations that both work: a `scanned_fn` source relation, and a `thing`
derived relation with two rules (one reading `scanned_fn`, one self-referencing).
Confirm it runs without the bail.
