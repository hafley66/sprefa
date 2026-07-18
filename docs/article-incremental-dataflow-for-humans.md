# Incremental dataflow for humans: weights, deltas, and the non-resident shape

An essay for practitioners who would like their tools to stop recomputing the world, and who have ten minutes. One small example runs through the whole piece: a call graph, a reachability view over it, and a single edit that removes one edge.

## The problem

Every build tool, linter, and code index faces the same question. You computed an answer from some inputs. One input changed. What is the cheapest way to make the answer correct again?

Two bad answers bound the space. The first is recompute the world: throw the answer away and rebuild it. Correct, simple, and slow in proportion to everything instead of in proportion to the change. The second is keep everything in RAM: maintain a live dependency graph in memory, invalidate the affected nodes when an input changes, and recompute only those. Fast, but the graph itself costs memory, and on a machine asked to hold hundreds of repositories, memory is the thing you do not have.

The good answer is older than either extreme and comes from databases. Treat every derived answer as a view — a query result stored so it can be read without re-running the query — and maintain the view incrementally: when the base data changes, compute only the difference and apply it to the stored result. The rest of this essay is about how that difference is computed, why deletion is the hard half of it, and why the database, not the heap, is the right place to keep the state.

## Relations with weights

A relation is a set of rows. A call-graph edge table with rows like `(main, a)` and `(a, b)` is a relation. A query over relations produces another relation.

Now add one column: an integer weight per row. A positive weight means the row is present that many times, so `+1` is an ordinary insert. A deletion is the same row with weight `−1`. A relation carrying weights like this is called a Z-set, after the mathematicians' name for the integers. Updating a relation means adding a Z-set of changes to it, summing the weights of identical rows. These change-sets are called deltas.

This single trick makes deletion ordinary. The standard query operators — filter, join, union, aggregation — all have weighted versions that map input deltas to output deltas: weights in, weights out. A union adds weights; a join multiplies the weights of the rows it combines. So when a row is deleted upstream, its `−1` flows through the same query plan its `+1` once used, and every derived row it contributed to gets its weight decremented. When a derived row's total weight cancels to zero, the row is retracted — removed from the view. There is no special delete path. Retraction falls out of the algebra.

The weight also records the one fact that makes naive deletion wrong: how many independent ways a derived row can be derived. The example shows why that matters.

## A call graph with weights

Four functions: `main`, `a`, `b`, `c`. The extractor has produced an edge table, one row per call:

| caller | callee | weight |
|---|---|---|
| main | a | 1 |
| a | b | 1 |
| b | c | 1 |
| a | c | 1 |

Over this sits a derived view, `reach(x, y)`: `x` can reach `y` through one or more calls. The rule is recursive — `reach` contains every edge, plus `(x, z)` whenever `reach(x, y)` and `edge(y, z)` — and its full contents, with each row's weight counting the number of distinct paths that derive it, is six rows:

| x | y | weight | why |
|---|---|---|---|
| main | a | 1 | the direct edge |
| a | b | 1 | the direct edge |
| b | c | 1 | the direct edge |
| a | c | 2 | direct edge, and a→b→c |
| main | b | 1 | main→a→b |
| main | c | 2 | main→a→c, and main→a→b→c |

Two rows have weight 2. Those two rows are the point of the exercise.

## The edit: retraction by cancellation

Someone edits the file containing `a` and deletes the call to `b`. Extraction emits one delta: edge `(a, b)`, weight `−1`.

The view update proceeds in rounds, each round feeding the previous round's deltas through the recursive rule.

Round 1. The `−1` on edge `(a, b)` matches the `reach` row `(a, b)` directly: emit `(a, b) −1`. It also extends: every row `(x, a)` in `reach` combined with that edge to produce a path `(x, b)`, so each loses one derivation. `reach` holds `(main, a)` with weight 1, so emit `(main, b) −1`.

Round 2. The paths retracted in round 1 end at `b`, and `b` has one outgoing edge, `(b, c)`. Each lost `(x, b)` loses a path to `c`: emit `(a, c) −1` and `(main, c) −1`.

Round 3. The new deltas end at `c`, and `c` calls nothing. There is nothing to extend. The round produces no deltas, so the update is finished.

Now sum the deltas into the view. `(a, b)`: 1 − 1 = 0, gone. `(main, b)`: 1 − 1 = 0, gone. `(a, c)`: 2 − 1 = 1, stays. `(main, c)`: 2 − 1 = 1, stays. The final view is `(main, a)`, `(a, c)`, `(b, c)`, `(main, c)` — exactly right, because `b` is now unreachable while `c` survives through the direct edge.

Watch what the weights did. Rows `(a, c)` and `(main, c)` each had two derivations; the edit destroyed one derivation of each, and the surviving derivation kept the row alive. A system that tracks only "this row depended on that edge" cannot express this. It either deletes both rows — wrong, they still hold — or keeps both — wrong only by luck. Its real options are to over-delete and then re-derive to repair the damage, or to keep a count. The count is the exact information required, and the count is what the weight column is. Delete the edge, and the algebra deletes precisely the paths that no longer exist.

## Semi-naive evaluation and progress

Even with deltas, evaluation can still be wasteful. The naive way to run the recursive rule each round is to join the entire current `reach` against `edge` looking for new paths. Semi-naive evaluation — the name is historical, the idea is not — joins only the previous round's deltas, the frontier of new knowledge, against the full tables. Nothing new can come from old rows joined with old rows; that work was done in earlier rounds. Cost per round becomes proportional to the change, not to the dataset.

Applied to the example, the whole update touched four derived rows, because the deltas were four rows. The rest of `reach` — in a real codebase, millions of rows — was never read. That sentence is the entire economic argument for incremental computation.

One bookkeeping question remains: how did the system know round 3 was the last? Each round must be complete before the next begins, and "complete" must be detectable. Differential dataflow, Frank McSherry's system built on the timely dataflow engine, solves this with logical timestamps: every delta carries one — here, the round number — and every operator maintains a frontier, the earliest timestamps from which further deltas might still arrive. When an operator's input frontier has advanced past round *k*, no more round-*k* deltas can appear, so its round-*k* output is final. Progress tracking is what lets a parallel or distributed system say "this round is done" without a global stop-and-ask. On a single thread inside one database the same fact is nearly free: the current delta stream is empty, so the fixpoint loop stops. The stop condition is the same either way; frontiers are how you learn it when the work is spread out.

## Two runtimes, one algebra

The algebra above — weighted relations, deltas, frontiers — was worked out most fully in differential dataflow and generalized to arbitrary recursive queries by DBSP, the formalism implemented in Feldera's engine. These systems get the math right. Their performance model rests on arrangements: sorted, indexed, shareable in-memory copies of each relation, kept ready so joins never scan. Arrangements are why differential dataflow is fast. They are also RAM-resident indexes over intermediate results, which is the wrong runtime when the goal is hundreds of repos on one machine and the RAM budget is already spent. DBSP's engine has an opt-in storage layer that can spill its state to disk, but it is off by default and brings its own configuration and tuning surface; out of the box, the traces live in memory. Salsa, the incremental framework behind rust-analyzer, makes the same residency choice in a different shape: it memoizes function results — stores each call's result keyed by its inputs — in an in-process dependency graph. Excellent for one live workspace; not designed for state that must mostly live on disk.

The same algebra exists in an older costume. Database researchers call the whole topic incremental view maintenance, IVM: keep a stored view correct under inserts and deletes by computing deltas instead of re-running the query. DBToaster showed you can compile a query into higher-order delta queries — standing queries that compute the delta of the view from the delta of each input — and keep the view itself in ordinary tables. pg_ivm implements a practical version inside PostgreSQL: declare a view as incrementally maintained and triggers on the base tables keep it current, with all state living in the same tables as everything else. In this lineage the disk carries the state, the buffer cache decides what is hot, and nothing assumes the working set fits in memory.

The math, in short, is settled. The choice is the runtime: arrangements in RAM, or tables on disk.

## The non-resident shape

This repository already made that choice, and it is worth naming precisely. Facts live in SQLite. Derived relations are built by a SQL fixpoint: rules run as insert-select statements, semi-naive over delta tables, looping until a round inserts nothing — the empty-frontier stop condition from two sections ago. Values are never assumed to be in RAM; SQLite's pager and the operating system's page cache decide what is resident. That is the non-resident shape: the derivation state is the database.

Reactivity today is coarse-grained. The engine persists rule-shape digests — hashes recording which derived relations depend on which source relations — and that digest graph is a small, storable trigger graph. When files change, the dirty source relations mark their dependents dirty, and a scoped rebuild deletes and re-derives the affected relations wholesale. The literature calls this DRed, delete-rederive: over-delete everything that might depend on the change, then re-derive what still holds. DRed is correct, and at relation grain it is simple. But it recomputes every unaffected row of every touched relation — the `(a, c)` rows of the world get deleted and re-derived even though only one of their derivations died.

The upgrade path is grain, not architecture. Add a weight column to a derived relation and the relation becomes a Z-set: retraction becomes a `−1` flowing through the same SQL the `+1` used, the over-delete half of DRed disappears, and exactly the dead rows vanish — the four-row update from the example instead of a relation rebuild. Nothing about residency changes; the weights live in the table, on disk, beside everything else. The digests keep doing the coarse routing they already do. Weights refine what happens inside the relations the digests mark dirty.

The honest tradeoff, stated once: for any derived relation, you can have zero stored view, correct retraction, or no recompute — pick two. Keep nothing and re-run the rule at read time: no storage, no wasted maintenance, but every read recomputes. Store the view without weights and rebuild it wholesale on change: correct results, but maintenance cost proportional to the relation. Or store the view with weights and maintain it by deltas: a little more storage, correct retraction, no recompute. In an engine built this way the choice is a per-relation knob, not a global one — weight columns where edits are frequent and re-derivation is expensive, plain rebuild where relations are small. The algebra does not care which you pick. The disk does not care. The workload decides.

---

## Part II

Part I made two claims: weights make deletion ordinary, and the derivation state belongs on disk. This part shows the algebra behind the first claim, takes one short detour into what "pathfinding" means when the graph is not in memory, and ends with a map of how tightly each system in the space is coupled to RAM.

## The DBSP algebra, actually.

DBSP — Database Stream Processor, the formalism implemented by Feldera — is usually presented in seventy pages of notation. The core is a handful of ideas, each simple. Here they are in order, over the same call graph, under the same edit.

**The object.** A Z-set is a table whose rows carry integer weights, and the point of the weights is that they make tables into something you can do arithmetic on. Add two Z-sets row by row, summing the weights of rows that match. Negate one by flipping every sign. The empty table is zero.

> **Definition.** The Z-sets over a given row type form a *group*: an addition that is associative, a zero, and a negative for every element. In plain terms — you can always add two tables of weighted rows, and you can always subtract. Nothing is ever undeletable.

Subtraction is the whole game, and the running example is one addition:

```
  { (main,a)+1, (a,b)+1, (b,c)+1, (a,c)+1 }     the edge table
+ { (a,b)-1 }                                    the edit
= { (main,a)+1, (b,c)+1, (a,c)+1 }               the new table
```

The delete was not a special operation. It was addition of a negative.

**Streams.** Now let time in. A *stream* is an infinite sequence of Z-sets, one per timestep, ticking forever. And here is the identification the whole theory rests on:

> A changing database IS a stream of deltas. At each tick, whatever edits arrived: timestep 0 delivers the four edges, timestep 1 delivers the retraction of `(a, b)`, timestep 2 delivers nothing — the zero Z-set — and the stream ticks on, mostly zeros, for as long as the database lives.

**Two operators.** On streams, two operators that undo each other. *Integration*, written I, is a running sum: position t of I(s) holds the sum of positions 0 through t of s. It replays deltas into the current state — what applying a commit log does. *Differentiation*, written D, is the adjacent difference: position t of D(s) holds position t minus position t−1. It turns states back into deltas.

Three timesteps of the example, all of it:

| timestep | delta arriving, d | state so far, I(d) | D(I(d)) |
|---|---|---|---|
| 0 | the four edges, +1 each | four edges | the four edges |
| 1 | (a,b) −1 | three edges | (a,b) −1 |
| 2 | nothing | three edges | nothing |

Compare the last column with the second. Identical — always, for any stream. D undoes I, and I undoes D. A history has two interchangeable descriptions: a log of changes, or a sequence of snapshots.

**Lifting.** Take any ordinary query Q — say `reach`. *Lifting* it, written lift(Q), means running it independently on every timestep of a stream: state in, state out, no memory between ticks. Lifting moves a query from tables to streams wholesale. Nothing clever has happened yet.

**The construction.** Assemble the pieces. The incremental version of Q is:

```
Q^inc  =  D ∘ lift(Q) ∘ I
```

Read right to left, in the order data flows: integrate the input deltas into states, run the full original Q on each state, differentiate the outputs back into deltas. Deltas in, deltas out.

This looks uselessly circular. It is — on purpose. I rebuilds the entire input at every step, lift(Q) recomputes the entire query on it, D subtracts consecutive answers: batch recomputation wearing a costume, saving nothing. DBSP says so itself, plainly. The construction is a specification — a precise statement of what the correct incremental answer even is. Its job is to exist so the next step has something to rewrite.

**The punchline.** The next step is algebraic rewriting: push I and D inward, through the individual operators inside Q, where they simplify or cancel. What happens depends on the operator's shape.

A *linear* operator distributes over addition: f(a + b) = f(a) + f(b). Filter, map, union, negation are linear. For these, the rewrites collapse to something almost funny: the incremental version of f is f. Deltas pass straight through. Feed a filter a four-row delta and it costs four rows of work; the million rows behind it are never read.

A *bilinear* operator — linear in each argument separately, the join being the standard example — obeys a product rule:

```
Δ(A ⋈ B)  =  ΔA ⋈ B  +  A ⋈ ΔB  +  ΔA ⋈ ΔB
```

The change in a product is the change on the left against the old right, plus the old left against the change on the right, plus the two changes together. This is the product rule from calculus, d(uv) = du·v + u·dv + du·dv, turning up uninvited in a discrete setting to govern database joins (the chain rule has a counterpart too); it is worth one quiet moment that the same algebra runs underneath both.

Watch it work. Let Q be edge ⋈ edge, the two-hop paths — (x, z) when edge(x, y) and edge(y, z). Both inputs are the edge table, so both deltas are the edit: {(a,b) −1}. Three terms:

- ΔA ⋈ B — the dead edge as first hop, extended by b's surviving edge (b,c): path (a, c) via b, weight −1.
- A ⋈ ΔB — the live edge into a, (main,a), extended by the dead second hop: path (main, b) via a, weight −1.
- ΔA ⋈ ΔB — (a,b) against itself; the first hop ends at b, and no edge in the delta leaves b. Empty, as it usually is: the product of two small deltas is smaller still.

Two deltas out — exactly the two two-hop paths the edit destroys, found by matching the delta against the tables, never by re-running the join. Cost scales with the change, not with state times state.

**Recursion.** The remaining operator is the fixpoint — the loop-until-nothing-changes that computes `reach`. DBSP handles it by letting one timestep contain a stream of its own: a stream of streams, the outer ticks carrying edits, the inner ticks carrying the refinement steps toward the fixpoint, with their own I and D inside. That is the one honest sentence you get here; the machinery is the middle of the paper, and it works.

**Closedness.** Count what is now on the table. Union, filter, map: linear, their own incremental versions. Join and its relatives: bilinear, product rule. Fixpoint: nested streams. That is the entire relational vocabulary. Every operator a datalog program can compose has an incremental form — and the incremental form of a composition is the composition of the incremental forms.

So the rewrite is mechanical. Take any datalog program, push I and D through it operator by operator, and out comes a delta program that is correct by construction. No operator left behind means no case left unhandled: the theory cannot leak. Most incremental systems offer the opposite experience — each operator's update behavior a special case, composition an act of hope. Here the guarantee is the theorem. You know it is going to work before you write the code.

That is the algebra Part I spent as prose. Weights, deltas, retraction-by-cancellation: not a trick but a group, not a heuristic but a rewrite with a proof behind it.

## Sidebar: what pathfinding means.

Pathfinding is route-finding through a graph — the shortest way from A to B. The classics: breadth-first search, which explores the graph in layers, one hop further each round, and finds shortest paths when edges are unweighted; Dijkstra's algorithm, which handles weighted edges by always extending the cheapest known route; and A*, which is Dijkstra plus a heuristic — an estimate of the remaining distance that steers the search toward the goal.

The Rust crate named pathfinding implements all three, and it matters here for one reason only: its algorithms take a closure — a function you supply, "give me a node, I return its neighbors" — instead of a graph object. The graph never has to exist in memory. The closure can answer each neighbor request with a SQLite query against the edge table on disk, and the search walks a graph that is never resident anywhere. For a call graph spanning a hundred repositories, that is the difference between feasible and not.

## How married is each system to RAM?

The algebra is agnostic about where state lives. The systems are not. The coverage map:

Differential dataflow. Arrangements — the sorted, indexed, shareable copies of each relation that keep its joins fast — are in-memory indexes by construction; residency is not an option but the design. The math is hosting-agnostic. The runtime is the arrangements, and it is not.

DBSP, algebra versus implementation. The algebra is representation-independent: a Z-set is weighted rows, and weighted rows can live in a heap, a file, or a table — the rewrites never ask. Feldera, the implementation, ships storage-backed operators that spill state to disk when it outgrows memory: off by default, but present.

DBToaster compiles a query into delta programs — standing queries that compute the view's delta from each input's delta, the product-rule rewrite generated mechanically. The compiled code keeps state to answer those queries, and the state is hostable in ordinary tables; nothing in the scheme insists on RAM.

pg_ivm inhabits the disk pole fully: triggers on the base tables, views stored as tables, every byte of state inside PostgreSQL, maintenance synchronous per statement — the view updates in the same transaction as the write.

Salsa is the deliberate contrast. A memo graph in RAM: cached function results keyed by their inputs, a dependency graph recording who called whom, invalidation by graph walk rather than by weighted delta. The keep-everything-in-memory pole from Part I, engineered to excellence, making no apology.

Side by side:

| system | incremental algebra | retraction | recursion / fixpoints | disk-hosted state | reactive daemon |
|---|---|---|---|---|---|
| Differential dataflow | ✓ | ✓ | ✓ | — | ✓ |
| DBSP / Feldera | ✓ | ✓ | ✓ | opt-in | ✓ |
| DBToaster | ✓ | ✓ | — | hostable | embeds |
| pg_ivm | ✓ | ✓ | — | ✓ | ✓ (PostgreSQL) |
| salsa | memoization | invalidation | ✓ | — | library |

Read the table twice. Down the first column: near-unanimity, because the algebra is settled — everyone has some form of incremental maintenance. Across the rest: divergence, almost all of it about hosting. No shipped row is full; no system today combines weighted deltas, correct retraction, recursive fixpoints, disk-hosted state, and daemon-style reactivity in one engine. A datalog-over-SQLite engine needs exactly that full row. The empty row in the table is the slot this repository occupies.

## Further reading

- DBSP, the paper: <https://arxiv.org/abs/2203.16684>
- Differential dataflow: <https://github.com/TimelyDataflow/differential-dataflow>
- Feldera: <https://github.com/feldera/feldera>
- DBToaster: <https://dbtoaster.github.io/>
- pg_ivm: <https://github.com/sraoss/pg_ivm>
- Salsa: <https://github.com/salsa-rs/salsa>
- pathfinding (Rust crate): <https://docs.rs/pathfinding>
