# The math behind the engine

A sub-series for [Reactive Facts](../README.md). The parent book builds intuition
for an incremental, reactive datalog engine over code facts. This sub-series gives
the math that makes those chapters precise: why the fixpoint settles, why one
recursive query computes shortest-path and reachability and counts depending only
on a swapped operator, why negation needs layers, and why incremental maintenance
is a derivative.

Read it after chapter 7 of the parent book, or pull a single chapter when a term
in the engine code (`tarjan`, `stratify`, `rebuild_derived`) stops being a name and
starts being a question.

> Read these in VS Code's Markdown Preview (Cmd+Shift+V). The footnotes, tables,
> and fenced pseudo-TypeScript render cleanly there; the pseudo-TS carries the math
> in its comments, so it is meant to be read, not skimmed.

## Chapters

1. [Order and lattices](01-order-and-lattices.md) — posets, join/meet, complete lattices, top and bottom; a lattice is how you merge two partial answers without ambiguity.
2. [Fixpoints](02-fixpoints.md) — monotone functions, ascending chains, Knaster–Tarski least fixpoint; datalog evaluation is climbing from bottom until `f(x) = x`.
3. [Semirings](03-semirings.md) — one recursive query, four properties (reach, shortest, count, bottleneck) by swapping `(plus, times)`; provenance semirings.
4. [Datalog on lattices](04-datalog-on-lattices.md) — values as lattice elements, monotone aggregation composes with recursion, non-monotone aggregation needs stratification; Dijkstra and Bellman–Ford as scheduled least fixpoints.
5. [Evaluation](05-evaluation.md) — naive vs semi-naive (the delta/frontier), stratified evaluation order; ties to `rebuild_derived` and `stratify`.
6. [Graph cores](06-graph-cores.md) — Tarjan's index/lowlink invariant, condensation to a DAG, seeded forward/reverse reachability, and what to do when DFS will not fit.
7. [Incremental](07-incremental.md) — wholesale recompute vs the derivative; DBSP's Z-sets and Differential Dataflow; borrowing the calculus without the RAM-resident model.
8. [Annotated bibliography](08-annotated-bibliography.md) — every citation, grouped, with a one-line "why read this," plus the Dover shelf.

## The running example (same as the parent book)

A tiny codebase: five files, six functions, one cycle, one sink, one dead function.

```
   main ──▶ run ──▶ parse ──▶ lex
             │        ▲          │
             │        └──────────┘     (run, parse, lex form a cycle)
             ▼
            log                         (a sink: calls nothing)

   helper                              (defined, calls nothing, called by nobody)
```

Every chapter asks the same questions of it: who reaches whom, what merges when two
derivations agree, which functions are unused, what is a cycle, what happens when
you edit one file.
