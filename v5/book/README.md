# Reactive Facts

A short book to build intuition for the thing you are actually building: an
incremental, reactive datalog engine over code facts, stored on disk, with
bounded memory. It starts from "what is a fact" and ends at how Glean and Zoekt
do it at scale and how your `dl` engine embodies the same ideas.

Read in order; each chapter depends on the previous one. Chapter 0 is the
exception: read it first, and again whenever the work feels like despair instead
of curiosity.

0. [A calmer frame](00-a-calmer-frame.md) — the arc of how this got built, v4 sorrow vs v5 calm, and biasing toward easier times. Read when you are in the pit.
1. [Facts, rules, and queries](01-facts-and-rules.md) — datalog from zero: facts, rules, joins, anti-joins, source vs derived.
2. [Recursion and the fixpoint](02-recursion-and-fixpoint.md) — transitive closure, the frontier, why it terminates.
3. [The cycle problem](03-the-cycle-problem.md) — self-support, why counting breaks, time vs structure, SCC condensation.
4. [Incremental maintenance](04-incremental-maintenance.md) — inserts vs deletes, ownership, the source/derived split.
5. [Where the bytes live](05-where-bytes-live.md) — storage engines vs databases, the bounded-RSS discipline, the zoo.
6. [Gold standards and your engine](06-gold-standards-and-your-engine.md) — Glean, Zoekt, and how `dl` is the same ideas.

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
