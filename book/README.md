# Reactive Facts

A short book to build intuition for the thing you are actually building: an
incremental, reactive datalog engine over code facts, stored on disk, with
bounded memory. It starts from "what is a fact" and ends at how Glean and Zoekt
do it at scale and how your `dl` engine embodies the same ideas.

Prefer to learn by writing programs rather than reading theory? The [hands-on tutorial track](tutorial/README.md) is the practical counterpart to this theory track: you build a tiny fixture repo and type real `.dl` programs against it, one lesson at a time.

Read in order; each chapter depends on the previous one. Chapter 0 is the
exception: read it first, and again whenever the work feels like despair instead
of curiosity.

<!-- BEGIN: book-index -->
0. [A calmer frame (read this when you are in the pit)](00-a-calmer-frame.md) — the arc of how this got built, v4 sorrow vs v5 calm, and biasing toward easier times. Read when you are in the pit.
1. [Facts, rules, and queries](01-facts-and-rules.md) — datalog from zero: facts, rules, joins, anti-joins, source vs derived.
2. [Recursion and the fixpoint](02-recursion-and-fixpoint.md) — transitive closure, the frontier, why it terminates.
3. [The cycle problem](03-the-cycle-problem.md) — self-support, why counting breaks, time vs structure, SCC condensation.
4. [Incremental maintenance](04-incremental-maintenance.md) — inserts vs deletes, ownership, the source/derived split.
5. [Where the bytes live](05-where-bytes-live.md) — storage engines vs databases, the bounded-RSS discipline, the zoo.
6. [Gold standards, and your engine](06-gold-standards-and-your-engine.md) — Glean, Zoekt, and how `dl` is the same ideas.
7. [The fast paths: the loops that make it scale](07-the-fast-paths.md) — the loops that make it scale: semi-naive fixpoint, Tarjan/condensation, seeded reachability, stratification, auto-index, with citations and exercises.
8. [Argmax and friends](08-argmax-and-friends.md) — max vs argmax, the candidates/beaten/winner negation shape, per-key vs per-row grouping, the `key(...) merge(MaxBy(...))` lattice shortcut, and the SQL it lowers to.
<!-- END: book-index -->

For the math behind these chapters (lattices, fixpoints, semirings, evaluation, graph cores, incremental maintenance), see the [math sub-series](math/README.md).

Two side surveys map the neighbouring territory:

- [Logic language survey](logic-language-survey/README.md) — where `dl` sits among Prolog, Scryer, Datalog, Soufflé, Mercury, and Ciao, on the two axes of run-direction and compiler-knowledge.
- [Beyond manual paging](beyond-manual-paging/README.md) — the algorithm theory the bounded-RSS frame hides: reachability labeling, succinct structures, graph sketches, cache-oblivious layouts, and delta-as-default maintenance.

The [further-study backlog](further-study.md) lists the concepts still worth their own survey: worst-case optimal joins, CFL-reachability, Roaring bitmaps, demand- vs change-driven incremental, and more.

## The running example (used in every chapter)

A tiny codebase. Five files, six functions, one cycle, one sink, one dead function.

```
a.rs:   fn main()  { run(); }
b.rs:   fn run()   { parse(); log(); }
c.rs:   fn parse() { lex(); }
        fn lex()   { run(); }      // lex calls back into run
        fn helper(){ }             // defined, never called
d.rs:   fn log()   { }
```

The call edges (caller -> callee):

```
   main ──▶ run ──▶ parse ──▶ lex
             │        ▲          │
             │        └──────────┘     (run, parse, lex form a cycle)
             ▼
            log                         (a sink: calls nothing)

   helper                              (defined, calls nothing, called by nobody)
```

Hold this picture. Every chapter asks a question about it: who reaches whom,
which functions are unused, what is a cycle, what happens when you edit `b.rs`.

## How to use it

Each chapter has the same shape:

- **The question** it answers.
- **Build-up** on the running example, with diagrams.
- **Intuition** — the one sentence to remember.
- **Exercises** with answers at the bottom, so you can self-check.
- **In your engine** — where this lives in the `dl` code you already wrote.

The goal is not to memorize algorithms. It is to be able to look at any
incremental/graph/datalog system and immediately see which of these few ideas it
is using, and which trade-off it picked.
