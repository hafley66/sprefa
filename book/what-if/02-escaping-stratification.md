# 2. Escaping stratification

> when the stratifier says no: add time, find the lattice, or split the cycle — and the semantics upgrades deliberately left on the shelf.

**The question.** Stratification (book chapter 8, tutorial lesson 5) demands
that negation and aggregation sit in layers: a relation may negate only
relations fully computed below it. Real programs keep colliding with that
wall. Rule A wants to suppress rows of B, B wants to suppress rows of A; a
tree render wants to aggregate its own output one level down; a "latest
config wins" rule wants to read the very relation it feeds. The checker
refuses the cycle. What are the exits?

There are four, and the engine already contains the first three in embryo.
The what-if is not building them; it is admitting they are one story.

## Exit 1: time

The oldest answer in the literature (Datalog with explicit timesteps, the
Dedalus line of work): give every fact a time argument, let the "cycle" cross
from tick T to tick T+1, and every program stratifies, because nothing at
time T depends on time T. `dl` ships this as `@next`:

```dl
etag(endpoint, tag) <- @next etag_next(endpoint, tag).
```

`etag_next` may depend on `etag` through any tangle of negation, because the
carry breaks the loop at the tick boundary; today's `etag` is yesterday's
`etag_next`, a finished fact. The engine even documents the resulting
awkwardness honestly: the *static* cycle checker over-flags `@next` loops it
cannot see are temporal (`--check` on `examples/gh-cache.dl`), while the tick
runs them correctly. A stratifier that understood "this edge crosses a tick"
would make exit 1 first-class instead of grandfathered: mark the edge, drop
it from the static graph, done. That is a checker change, not an evaluator
change.

The general recipe: **a paradox is a process**. "The value depends on itself"
almost always means "the next value depends on the current one," and `@next`
is that sentence spelled as a rule.

## Exit 2: lattices

Stratification exists to protect non-monotone operations: negation can
*retract* a conclusion when a new fact arrives, so it must wait for its input
to finish. But many uses of negation are monotone selection wearing a
disguise. Lesson 8's dispatch is the worked example: the naive spelling
guards the fallback with `!handled(id)` (negation, needs a stratum); the
lattice spelling offers every row and lets `key(id) merge(MaxBy(prio))` keep
the winner. Merge functions like max only ever move *up* a lattice, and
monotone rules may recurse freely, no layers required. This is the Flix /
Datafun family's core insight, and the math companion derives it properly
(`book/math/04-datalog-on-lattices.md`).

Today `key()/merge()` applies per relation declaration. The unexplored
headroom: recursion *through* a merge. "Shortest path" is `merge(MinBy(dist))`
with a rule that reads its own relation; the fixpoint is classic and
terminates for the same lattice reasons. The declaration syntax already says
everything the evaluator would need to know.

The recipe: **before reaching for `!`, ask what the winning row is**. If the
negation exists to pick a best/latest/highest row, it is an argmax, and an
argmax is a lattice, and a lattice needs no stratum.

## Exit 3: structure

Sometimes the cycle is real but the *data* is acyclic. The tree render in
[the previous essay](01-rendering-trees.md) recurses through an aggregate,
which the stratifier refuses in general — a cyclic `child` would make the
fold genuinely undefined. But `child` over a tree is a DAG, and over a DAG
the fold visits each node once, leaves first. The engine already owns every
piece of this: Tarjan condensation (book chapter 3), per-component evaluation
order (`rel_components` splits each stratum and runs acyclic components in
one pass), and `scc(edge)` as a queryable relation. A "stratify by the data,
not the rules" mode is: condense the *value* graph, check the offending
operation only ever crosses condensation layers, evaluate in topological
order. Refuse loudly when a cycle appears in the data at tick time — the same
honesty contract as the lattice-mixed-relation bail (a `key(...)`/`merge(...)`
relation also headed by a source or derived rule refuses rather than guess
which side wins a key collision).

The recipe: **the rule graph is a conservative approximation**. When the
checker refuses, the actual data often carries a proof of layering the rules
cannot express. Making that proof checkable (acyclicity as a declared,
tick-verified property of a relation) is the smallest version of this exit.

## Exit 4, declined: stronger semantics

The literature's remaining door is changing what a program *means*:
well-founded semantics gives every program an answer by allowing "unknown,"
and stable-model semantics (answer-set programming) allows several answers
and hands you the set. Both are principled and both are wrong for this tool.
A reactive engine whose diagnostics gate commits cannot print "unknown," and
a codemod cannot apply one of three models. The three exits above share the
property that matters: the answer stays one definite set of rows, and the
program that needs a stratum it cannot have is reshaped, not reinterpreted.

## The unified sentence

When stratification refuses: the dependency is temporal (add `@next`), or it
is a selection (declare the lattice), or the data is layered even though the
rules are not (condense and fold). If it is none of those, the program is
probably trying to say something it does not mean.
