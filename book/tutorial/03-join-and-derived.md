# 3. Join and derive

> two source relations joined into a third derived one, and what happens when one relation is headed by both a source rule and a derived rule.

**Goal:** join two source relations into a third derived one, and see how the
engine handles a relation written both ways.

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

## One relation, mixed rule kinds

Naively, a relation headed by both a source rule and a derived rule looks like
trouble: rebuilding derived relations clears them first, which would wipe out
the scanned rows underneath. Try it anyway. Save as `03-mixed.dl`:

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
dl 03-mixed.dl --no-daemon
```

This runs. Under the hood the engine splits `thing` into two hidden relations
you never see or name yourself — one that gets every source rule's rows, one
that gets every derived rule's rows — and unions them back into `thing` for
everything else in the program (this `? thing(name, line).` query included) to
read under its original name. The scanned rows survive every derived rebuild;
the self-recursive rule still reaches its rows through the same `thing` name.
This is the exact split you would write by hand — a scanned relation, a
derived relation, a union — done for you.

Two combinations still refuse outright rather than desugar, because a silent
answer would be the wrong answer: a `key(...)`/`merge(...)` lattice relation
mixed this way (which side wins a key collision is not decidable by the
engine), and an `@in`/`@out` port relation headed by anything but its own
serving loop.

## Exercise

Rewrite `03-mixed.dl` by hand into two relations that behave the same way: a
`scanned_fn` source relation, and a `thing` derived relation with two rules
(one reading `scanned_fn`, one self-referencing). Confirm both versions
produce the same rows — the manual split and the automatic desugar are the
same shape.
