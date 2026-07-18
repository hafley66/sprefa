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

## Further reading

- DBSP, the paper: <https://arxiv.org/abs/2203.16684>
- Differential dataflow: <https://github.com/TimelyDataflow/differential-dataflow>
- DBToaster: <https://dbtoaster.github.io/>
- pg_ivm: <https://github.com/sraoss/pg_ivm>
- Salsa: <https://github.com/salsa-rs/salsa>
