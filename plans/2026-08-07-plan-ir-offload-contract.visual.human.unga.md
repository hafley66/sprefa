# Getting SQL out of the compiler's first pass: the plain version

No citations. No line numbers. Just what the thing is, why, and what we do next.

## TOC

1. [What we have now](#1-what-we-have-now)
2. [What is wrong with it](#2-what-is-wrong-with-it)
3. [The cut we are making](#3-the-cut-we-are-making)
4. [What goes in the box (and what stays out)](#4-what-goes-in-the-box-and-what-stays-out)
5. [Who runs the box: the shopping trip](#5-who-runs-the-box-the-shopping-trip)
6. [Where it plugs in](#6-where-it-plugs-in)
7. [The order thing (this is the scary one)](#7-the-order-thing-this-is-the-scary-one)
8. [How much faster, really](#8-how-much-faster-really)
9. [The plan, as lanes](#9-the-plan-as-lanes)
10. [What could go wrong](#10-what-could-go-wrong)

---

## 1. What we have now

You write a `.dl6` program. A prolog compiler turns it into a TypeScript file.
That TypeScript file talks to SQLite.

```
  your program            the compiler                   what runs
  ────────────            ────────────                   ─────────

  reachable(x,y) <-       parse                          a .ts file full of
    edge(x,y).       ──►  figure out types          ──►  SQL STRINGS, and a
  reachable(x,z) <-       build a plan                    driver that feeds
    reachable(x,y),       ...bake SQL into it             them to SQLite
    edge(y,z).
```

The plan is a real structured thing. It has names and shapes: a level statement,
an expand plan with 7 slots, a DRed plan with 24 slots, a refCount tuple with 11
slots. That is a good plan.

The problem is what is *inside* each slot.

```
   the plan (good, structured)          each slot (bad, a string)
   ┌────────────────────────┐
   │ dredplan/24            │           slot 5 = "INSERT OR IGNORE INTO
   │  ├ clearPing ──────────┼──────►      \"__pong_reachable\" (\"x\",\"z\")
   │  ├ clearPong           │             SELECT \"x\",\"z\" FROM (SELECT
   │  ├ clearCone           │             b0.\"x\", b1.\"y\" FROM \"__ping_
   │  ├ assertSeeds[]       │             reachable\" b0, \"edge\" b1 WHERE
   │  ├ assertHopAB ────────┼──────►      b0.\"y\" = b1.\"x\") x WHERE NOT
   │  ├ ...                 │             EXISTS (SELECT 1 FROM ...)"
   │  └ headCount           │
   └────────────────────────┘            ^^^ SQLite, forever, baked in
```

## 2. What is wrong with it

Three things.

**One.** The compiler can only ever target SQLite. Want a different engine?
Rewrite the compiler.

**Two.** SQLite has a hard speed limit for this workload, and we measured it
three separate times. It derives about **1 million rows per second**, and that
is the cost of writing a row into a sorted tree. Not the join. Not the loop. The
write. We tried five different batching and storage tricks. Every one lost or
tied.

**Three.** Rust does the same work between 5 and 68 million rows per second. We
have three rust engines sitting in the repo right now proving it.

```
  derived rows per second, chain_10000, all measured, same machine

  mono (rust, generated code)   ████████████████████████████  68.5M
  rxgraph (rust, wired graph)   ███████████████████████       56.2M
  interp (rust, generic)        ███                            7.3M
  raw SQLite, no dl machinery   ▌                              1.1M
  dl6 today                     ▏                              0.3M
```

That bottom bar is us. That is the gap.

## 3. The cut we are making

Instead of the compiler writing SQL strings, it writes a **description of the
work** with no SQL in it. Then a second step turns that description into
whatever engine you like.

```
  BEFORE

  compiler ──► plan with SQL strings inside ──► SQLite. only SQLite. ever.


  AFTER

  compiler ──► plan with a NEUTRAL DESCRIPTION inside ──┬──► SQLite (today)
                                                        └──► rust (new)
```

The neutral description is called the IR. It is a small vocabulary:

```
   scan a table        join two things       filter rows
   read a delta        project a row         is this row missing?
   insert if new       append                clear a scratch table
   delete matching     count                 loop until nothing new
```

Twelve words. That is the whole thing. None of them say "SQLite".

## 4. What goes in the box (and what stays out)

We are NOT converting the whole compiler. We are converting the one part that
eats all the time: **the recursive loop**.

```
  ┌─────────────────────────────────────────────────────────────┐
  │  A TICK                                                     │
  │                                                             │
  │  arrivals ─► edge rules ─► ┌──────────────────┐ ─► staging  │
  │                            │  THE RECURSIVE   │    deltas   │
  │                            │  FIXPOINT LOOP   │    frontier │
  │                            │                  │    tick log │
  │                            │  <-- THIS BOX    │    boundary │
  │                            │      MOVES       │             │
  │                            └──────────────────┘             │
  │                              75% of the time       25%      │
  │                                                    stays    │
  └─────────────────────────────────────────────────────────────┘
```

Two lucky breaks make that box much smaller than you would fear:

**Lucky break 1: no math.** On the recursive path, the refCount column is
literally the number 1. Every single time. So the IR needs zero aggregation. No
count, no sum, no min, no max, no average. Gone.

**Lucky break 2: the fence already exists.** The compiler already refuses to
build a DRed plan when a rule has a negated atom, or a snapshot read, or a
struct dictionary. So the hard cases already fall back to the slow-but-correct
path, and they will keep doing exactly that. We inherit a tested gate instead of
inventing one.

Everything else stays in SQL for now, and that is fine:

| stays in SQL | why that is fine |
|---|---|
| JSON digging | it is a join, not a loop; nobody's bottleneck |
| regex | a filter is never the slow part |
| count / sum / min / max / average | different maintenance shape entirely |
| text rendering at the boundary | that IS the SQLite contract, on purpose |
| retention, catalog, tick table | metadata, a handful of rows |

## 5. Who runs the box: the shopping trip

The rule in this repo is: never say "write our own" for a common problem
without shopping first, candidate by candidate. So we shopped.

| candidate | what it is | why not |
|---|---|---|
| **DataFusion** | Arrow columnar SQL engine | built for scanning big columns; our job is 10M tiny lookups. Recursive queries are off by default because they can eat all your memory. No retraction at all. Huge dependency |
| **differential-dataflow** | incremental dataflow, the real deal | genuinely the best idea here. Correct on cycles *by theorem*, not by heuristic. **But** we measured it: it keeps everything in RAM, ~215 bytes per node, 618 MB at 2.9M nodes. We already peak at 4 GB. It also brings its own notion of time, which throws away our byte-for-byte grading |
| **ascent / crepe** | datalog as a rust macro | rules must be known when rust compiles. We compile user programs at runtime. There is a runtime interpreter version, and it is version 0.1.2 of a brand new crate doing what our own 578-line `interp` already does at measured speed. Neither has any retraction story |
| **DuckDB** | fast columnar database | it speaks SQL. We are trying to *stop* speaking SQL in the compiler. Also a C++ build in-process |
| **Turso** (ex-Limbo) | SQLite rewritten in rust | also speaks SQL, so same objection. Also explicitly in beta, and its own maintainers say be careful. Separately: swapping libsql for Turso is a totally different decision and this plan does not block it either way |
| **our own** | `interp` + `rxgraph` + `sprefa-store`, all in the repo | ✅ |

**We picked our own.** Not out of pride. Out of one number.

```
  rxgraph: "the program is a graph of boxed operator objects wired at startup"

  That IS an IR interpreter. It already exists. It already runs.

  chain_10000      rxgraph 56.2M  vs  mono 68.5M   ← 82% of generated code
  chain_1000000    rxgraph 23.5M  vs  mono 19.4M   ← FASTER than generated code
```

The lab that was built to measure "how much does dynamic dispatch cost" answered
"18%, and at scale, nothing." An engine wired from data at load time is not
slow. That is the whole permission slip.

And the retraction algorithm is already written in rust, tested, in
`sprefa-store`. Including the dead ends: we already measured that the "clever"
recursive-CTE version is 20% slower than the boring loop.

**What we buy vs what we build:**

```
  BUY:   hash maps (rustc-hash)     BUILD:  the datalog fixpoint walk
         JSON reading (serde)               ...and that is it
         process lifetime (the OS)
```

## 6. Where it plugs in

Three ways to attach it. We picked the middle one.

```
  (a) EVERYTHING through the new box
      ┌──────────────────────────────────┐
      │ every statement becomes IR       │  ✗ now you must support JSON,
      │ arrivals, edges, aggregates,     │    regex, collation, aggregates,
      │ boundary, retention, all of it   │    text rendering... unbounded
      └──────────────────────────────────┘

  (b) ONE BRANCH, per level, opt-in           ← WE PICKED THIS
      ┌──────────────────────────────────┐
      │ the driver already picks between │  ✓ one more `if` at a spot that
      │ 3 paths today. add a 4th.        │    already picks between 3 things
      │ everything else untouched.       │  ✓ no IR? runs exactly as today
      └──────────────────────────────────┘

  (c) A WHOLE SECOND RUNTIME
      ┌──────────────────────────────────┐
      │ a second emitted target that     │  ✗ throws away 211 fixtures worth
      │ does not use the driver at all   │    of proof, starts from zero
      └──────────────────────────────────┘
```

The branch, drawn:

```
                    reconcileRefCountStatement
                              │
                    ┌─────────┴─────────┐
                    │ does this level   │
                    │ carry an IR plan? │
                    └─────────┬─────────┘
                     no │           │ yes
              ┌─────────┘           └──────────┐
              ▼                                ▼
    ┌──────────────────┐              ┌──────────────────┐
    │ the 3 paths      │              │  RUST FIXPOINT   │
    │ that exist today │              │  give me rows    │
    │ (unchanged)      │              └────────┬─────────┘
    └────────┬─────────┘                       │
             │                                 │
             └────────────┬────────────────────┘
                          ▼
              ┌───────────────────────────┐
              │ THE SAME TAIL STATEMENTS  │
              │ stage the deltas          │
              │ stage the frontier        │
              │ write the tick log        │
              │ (nothing changes here)    │
              └───────────────────────────┘
```

## 7. The order thing (this is the scary one)

Our test suite does not just check *which* rows came out. It checks *what order
the events happened in*, byte for byte, against a prolog reference engine. 211
fixtures. If the order shifts, everything goes red.

So: where does the order come from today?

```
  Two SQL facts, and only two.

  FACT 1                                FACT 2
  ──────                                ──────
  the scratch table __new_<rel>         every wavefront table is
  has no primary key, so rows           WITHOUT ROWID on the head key,
  keep their insertion number.          so scanning it comes out SORTED.

  the event's sequence number
  IS that insertion number.
```

Put the two together and you get two different, precise, testable rules:

```
  EXPAND PATH                        DRED PATH
  ───────────                        ─────────
  one big pass at the end            one write per round of the walk

  ┌────────────────────────┐         ┌──────────────┐
  │ all rows, sorted by    │         │ round 0, sorted│
  │ the head key           │         ├──────────────┤
  │                        │         │ round 1, sorted│
  │  a  b  c  d  e  f  g   │         ├──────────────┤
  └────────────────────────┘         │ round 2, sorted│
                                     └──────────────┘
  "key order"                        "round order, sorted inside"
```

So the contract for the rust box is one sentence:

> **Give the rows back in that exact order, and the existing SQL statement
> numbers them, and the test suite never notices you were there.**

That is why option (b) is safe and (c) is not. The numbering stays in SQL.

## 8. How much faster, really

This is the part where a plan usually oversells. Not here.

`chain_10000` cold build, today, 30.7 seconds. Where does it go?

```
   ████████████████████████████████████████████████████░░░░░░░░░░░░░░░░
   └────────────── the loop, 23.0s ──────────────┘└─ the tail, 7.6s ─┘
                          75%                              25%
                    ← THIS MOVES →                    ← this stays →
```

The loop goes from 23 seconds to about 1.4 seconds. The tail stays at 7.6
seconds, because the tail is writing 10 million rows into SQLite and that is the
1-million-rows-per-second wall.

```
   TODAY        ████████████████████████████████████  30.7s

   PHASE 1      ██████████░                            ~9-12s     3x
                └ tail ──┘└loop

   raw SQLite   ██████████                             ~9.2s   (the wall)

   PHASE 2      ██                                     ~2.5s    12x
                (head lives in rust, SQLite only sees
                 what somebody actually reads)

   rust anchor  █                                       1.4s
```

**Phase 1 gets you to the SQLite wall. It does not get you past it.** Getting
past the wall means the rows stop living in SQLite, and that is phase 2, and
that is a separate plan.

Anyone who reads this and expects 20x from phase 1 has misread it. That
sentence is in the real doc too.

## 9. The plan, as lanes

**Phase 0, measure before building. Two lanes.** Because two of the numbers
above are arithmetic, not measurements.

```
  P0-A  how much does it cost to turn 4 text columns into integers,
        10 million times?  (all our fast numbers used integer keys.
        real programs use text.)                              [opus]

  P0-B  time JUST the tail. pre-compute 10M rows, then just do
        the inserts. if that alone is over 12 seconds, phase 1
        cannot hit its target and we stop and re-plan.       [flash4]
```

**Phase 1, build it. Five lanes, disjoint files.**

```
  P1-A  prolog: build the IR terms, emit them as JSON              [opus]
        nothing reads them yet. tests must not move at all.

  P1-B  typescript: declare the types, plus a SLOW reference       [flash4]
        interpreter in TS that produces the same rows.
        this is the referee that catches drift.

  P1-C  rust: the new crate. deserialize IR, wire the operator     [opus]
        graph, run the walk, return rows in the right order.

  P1-D  wire the branch in the driver. one `if`.                   [flash4]

  P1-E  a sweep mode that FORCES the new path on every fixture     [opus]
        that has an IR plan, and diffs byte for byte.
```

Who owns what, so nobody collides:

```
  P1-A ──► lower.pl, emit_ts.pl          (prolog only)
  P1-B ──► types.ts, ir.ts               (typescript types only)
  P1-C ──► v6/sprefa-fixpoint/           (a brand new folder)
  P1-D ──► 1_incremental.ts              (one branch)
  P1-E ──► scripts/                      (one script)
```

**Every lane gets a second pass.** An implementation lane is not done when the
code works. It is done when a *different* agent has reviewed it and closed the
review. That is written into the plan as named review lanes, one per
implementation lane.

## 10. What could go wrong

Ranked. Number 1 is the one that matters.

**1. We move the wrong thing.** The loop is 75% of the time, so moving it feels
obviously right. But the remaining 25% is a hard floor made of disk writes, and
after the loop is free, that floor IS the number. If the floor turns out to be
higher than we think, we ship a beautiful correct IR that makes nothing faster.

→ *That is exactly what phase 0 lane B measures, and it is a stop-the-plan gate.*

**2. Two copies of the same logic drift apart.** The compiler will describe the
same rule twice: once as SQL (for the fallback) and once as IR (for the new
path). If a filter exists in one and not the other, the new path quietly derives
extra rows.

→ *Lane P1-B's slow reference interpreter exists only to catch this, and it runs
on every fixture, not a sample.*

**3. SQLite habits leak into the "neutral" IR.** Equality, null handling, text
sorting, integer division. If the IR says "these are equal" without saying whose
definition of equal, the two engines will disagree on some row somewhere.

→ *The compiler already refuses to join a text column to an integer one, and
already pins one storage type per column, so the IR carries the types and the
rust comparator is defined by them.*

**4. The order shifts and 211 tests go red at once.** See section 7.

→ *Two ordering rules, both written down, both tied to a specific line of table
definition, both graded by lane P1-E.*

**5. Text keys are expensive and we only ever benchmarked integers.**

→ *Phase 0 lane A, first thing, before anything is built.*

**6. Reloading data on every tick destroys the incremental numbers.** We
currently do a 1-million-row incremental tick in 42 milliseconds. If the rust
box has to be handed all the data every tick, that becomes seconds.

→ *Phase 1 is allowed to apply the new path to the COLD BUILD ONLY and leave
incremental ticks exactly as they are. The 42ms number is a gate, not a hope.*

---

## The one-paragraph version

The compiler builds a good structured plan and then ruins it by baking SQLite
strings into every slot. We cut those strings out and replace them with a
twelve-word neutral vocabulary, but only for the recursive loop, which is 75% of
the time and (lucky) needs no aggregation at all. A rust engine reads that
vocabulary and runs the loop. We looked hard at five libraries; the best one
(differential-dataflow) is correct-by-theorem but keeps everything in RAM and
brings its own clock, which would cost us our byte-for-byte test suite, so we
extend the three rust engines already sitting in the repo instead. It plugs in
as one extra branch at a spot that already branches three ways, and it hands
rows back in a precisely-specified order so the existing SQL keeps doing the
event numbering and no test notices. It gets us to the SQLite write wall, about
3x, and no further. Getting past the wall is phase 2. Phase 0 measures two
numbers first and is allowed to cancel phase 1.
