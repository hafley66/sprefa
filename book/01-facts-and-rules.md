# 1. Facts, rules, and queries

> datalog from zero: facts, rules, joins, anti-joins, source vs derived.

**The question:** what is the smallest set of pieces you need to ask questions
about code, and where does the answering happen?

## A fact is a row that is true

A fact is a named tuple you assert is true. From the running example, reading the
source gives you facts:

```
calls("main", "run")
calls("run",  "parse")
calls("run",  "log")
calls("parse","lex")
calls("lex",  "run")
def("main")  def("run")  def("parse")  def("lex")  def("helper")  def("log")
```

`calls` and `def` are **relations** (sets of facts of the same shape). That is
it. A relation is a table; a fact is a row. You already produce these by running
ast-grep / tree-sitter over the files.

## A rule derives new facts from existing ones

A rule says "if these facts hold, then this new fact holds." Shared variable
names across the body are the join:

```
edge(X, Y)  :-  calls(X, Y).            -- rename calls to edge (trivial rule)
sink(X)     :-  def(X), not calls(X, _).   -- X is defined and calls nobody
```

Read `:-` as "if". Read a comma as "and". A capitalized name is a **variable**;
`_` means "anything, do not bind it". Two atoms sharing a variable must agree on
its value, that is a join:

```
co_caller(X, Z)  :-  calls(X, Y), calls(Y, Z).
                          ▲   ▲
                          └───┴── same Y => "X calls something that calls Z"
```

`co_caller("main", "parse")` is derived because `calls("main","run")` and
`calls("run","parse")` share `Y = "run"`.

## Negation is a question about absence

`not calls(X, _)` keeps `X` only when **no** `calls` fact has `X` as caller. This
is an **anti-join**. It is how you ask "what is missing":

```
unused(X)  :-  def(X), not calls(_, X).   -- defined, but nobody calls it
```

Run it on the example: `helper` is defined and appears as nobody's callee, so
`unused("helper")`. Also `main` (nobody calls main). Everything else is some
edge's callee. The closed-world assumption is doing the work here: a fact that is
not derivable is taken to be false, so "absence" is a definite answer, not an
unknown.

## Source facts vs derived facts

Two kinds of relation, and the distinction is the spine of the whole book:

```
         the world (files)
              │  read + extract (ast-grep / regex / tree-sitter)
              ▼
   SOURCE facts:   def, calls          ← each one came from a file
              │  rules (joins, anti-joins)   no file is touched here
              ▼
   DERIVED facts:  co_caller, unused, sink   ← computed purely from other facts
```

A **source fact** has a home: the file it was read from. A **derived fact** has
no file; it was *computed*. This split decides everything about incrementality
later: source facts can be retracted by file (delete a file, delete its rows);
derived facts have to be recomputed or invalidated through what they depend on.

The dead-simple test for which kind a rule produces: **does its body read a
file** (a scan / pattern / extractor), or does it only mention other relations?
File-reading body = source rule. Relations-only body = derived rule.

## Intuition

> A fact is a true row. A rule is a join (shared variables) plus optional
> negation (absence). Source facts come from files and can be retracted by file;
> derived facts are computed and must be recomputed or invalidated. Everything
> else in this book is consequences of that last sentence.

## Exercises

1. Write a rule `reachable_in_one_or_two(X, Z)` for "X calls Z directly, or X
   calls something that calls Z."
2. Using the example, list every fact `co_caller(X, Z)` (X calls Y calls Z).
3. Why is `main` in `unused` under the definition `unused(X) :- def(X), not
   calls(_, X)`? Is that what you want? How would you exclude entry points?
4. Classify each as source or derived: `def`, `calls`, `unused`, `co_caller`.
   For each derived one, name the relations it joins.

## In your engine

In `dl`, `rel def(...)` / `rel call(...)` with a `scan(...) ... sg(...)` body are
**source rules** (they read files). `calls(caller,callee) <- def(...), call(...)`
and `unused(name) <- def(...), !call(...)` are **derived rules** (relations
only). Your `_prov(rel, path)` table records the file each source fact came from,
which is exactly "a source fact has a home." Chapter 4 is about what that home
buys you.

## Answers

1. `reachable_in_one_or_two(X,Z) :- calls(X,Z).` and
   `reachable_in_one_or_two(X,Z) :- calls(X,Y), calls(Y,Z).` (two rules, same
   head = union/OR.)
2. main→run→{parse,log}, run→parse→lex, run→log (none, log is a sink),
   parse→lex→run, lex→run→{parse,log}. So: (main,parse),(main,log),(run,lex),
   (parse,run),(lex,parse),(lex,log).
3. Because no `calls(_, "main")` fact exists. It is technically correct ("nobody
   calls main") but usually not what you want. Exclude entry points with another
   relation: `entrypoint("main").` and `unused(X) :- def(X), not calls(_, X), not
   entrypoint(X).`
4. `def`, `calls` = source (read from files). `unused` = derived (joins `def`
   anti-`calls`). `co_caller` = derived (joins `calls` with `calls`).
