# 3. The cycle problem

> self-support, why counting breaks, time vs structure, SCC condensation.

**The question:** computing the fixpoint from scratch handles cycles fine. But
the moment you want to *update* it when an edge is deleted, cycles become a trap.
Why, and how do real systems escape it?

## A fact can support itself through a cycle

Recall the cycle `run → parse → lex → run`. In `reaches`, `run→run` is derived:

```
run → parse → lex → run    proves    reaches(run, run)
```

Now ask: how is `reaches(run, parse)` justified? One justification is the direct
edge. But another is the loop: `reaches(run, lex)` and `calls(lex, run)` give
`reaches(run, run)`, and `reaches(run, run)` plus `calls(run, parse)` re-derive
`reaches(run, parse)`. A fact on a cycle participates in deriving **itself**.

That is fine when building up (the fixpoint just stops adding). It is poison when
tearing down.

## Why naive "counting" breaks

A tempting way to make deletion cheap: store, per derived fact, a **count** of
how many ways it was derived. Insert bumps the count; delete decrements; at zero,
remove the fact. This is the *counting algorithm*, and it is correct for
non-recursive views.

On a cycle it lies. Delete the edge `calls(lex, run)` (suppose `lex` stops
calling `run`). The truth: the cycle is broken, `run`/`parse`/`lex` no longer
reach themselves. But the counts of `reaches(run,run)` etc. were propped up by
the cyclic derivations, which still *look* present to a local counter, so the
counts never reach zero and the self-reaches never get retracted.

```
delete calls(lex, run)
   counting:  reaches(run,run) still has a "derivation" through the cycle
              that the counter can see → count stays > 0 → NOT removed → WRONG
```

The counter has no notion of "is this derivation grounded in something outside
the cycle." A cycle is a derivation with no base case, and a scalar count cannot
tell circular support from real support.

## Two principled escapes

This is the same wall every incremental engine meets. There are exactly two ways
through, and they map onto the two halves of this book.

**Escape 1 — order the iterations with TIME.** Differential dataflow / DBSP give
each round of the fixpoint a logical timestamp. The cycle `run→parse→lex→run`
unrolls into a *spiral*: `run` at iteration 3 is a different (fact, time) than
`run` at iteration 2, so there is no paradox; the math is over a well-founded
order, not a mutable counter. Cost: it keeps indexed copies of the relations
(arrangements) resident in RAM. Correct, powerful, memory-hungry.

**Escape 2 — make the structure acyclic first.** Collapse each cycle into a
single node, then there are no cycles left, and counting becomes sound. This is
the practical unlock, and it is cheaper on memory.

## SCC condensation: the practical unlock

A **strongly connected component** (SCC) is a maximal set of nodes that all reach
each other. In the example, `{run, parse, lex}` is one SCC; `main`, `log`,
`helper` are singletons. Condense each SCC to one super-node:

```
   original (has a cycle)              condensed (a DAG, no cycles)

   main ─▶ run ─▶ parse ─▶ lex         main ─▶ [run,parse,lex] ─▶ log
            │      ▲          │                      │
            │      └──────────┘                      ▼
            ▼                                        (singletons: main, log, helper)
           log
```

The condensed graph is always a **DAG** (no cycles, by construction). And on a
DAG, counting is *sound* — no fact can support itself, because there are no
cycles. So:

- Run cheap, sound **counting on the condensed DAG**.
- Reachability inside an SCC is trivial: every node reaches every other (that is
  what "strongly connected" means).
- `reaches(X, Y)` overall = "X and Y are in the same SCC" OR "X's SCC reaches Y's
  SCC in the condensed DAG."

You never store the full closure. You store the SCC partition and the small
condensed reachability, and reconstruct `reaches(X,Y)` by a join when asked. The
cycle problem is *confined inside an SCC and removed by condensation.*

## Intuition

> A fact on a cycle helps derive itself, so a scalar derivation-count cannot tell
> real support from circular support, and counting breaks on deletion. Escape it
> by ordering iterations with time (differential dataflow, RAM-heavy) or by
> condensing each cycle into one node so the graph becomes a DAG where counting
> is sound (cheap). Condensation is the memory-frugal choice.

## Exercises

1. List the SCCs of the running graph. Which are singletons?
2. Draw the condensed DAG. Confirm it has no cycles.
3. Delete `calls(lex, run)`. What are the SCCs now? What happened to
   `reaches(run, run)`?
4. Why is counting sound on a DAG but not on a general graph? State it in one
   sentence about base cases.

## In your engine

Your `reaches` is currently recomputed wholesale each change, which sidesteps the
cycle problem by never doing incremental deletion. The upgrade (Chapter 4 and the
research doc) is SCC condensation: store `scc_of(node, scc)` and a condensed
`scc_reach(scc_src, scc_dst, count)`, run counting on that DAG, and reconstruct
`reaches` as a view. That turns "rebuild the whole closure on every edit" into
"touch one SCC," at bounded memory, while staying correct on cycles.

## Answers

1. SCCs: {run, parse, lex} (the cycle), and singletons {main}, {log}, {helper}.
2. main → SCC{run,parse,lex} → log. helper isolated. No edge returns to a
   predecessor, so no cycle. It is a DAG.
3. Removing lex→run breaks the cycle. New SCCs are all singletons: {run},
   {parse}, {lex}, {main}, {log}, {helper}. `reaches(run,run)` (and parse, lex
   self-reaches) disappear — there is no longer any cycle, so nothing reaches
   itself. This is exactly the deletion counting got wrong and condensation gets
   right (you would re-run SCC detection on the affected component).
4. On a DAG every derivation chain bottoms out at a base fact (a source edge)
   because you cannot return to a node you came from; on a cyclic graph a chain
   can loop forever with no base, so a count can be positive with no grounded
   derivation.
