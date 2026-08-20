# graph_lowering CONTRACT: graph algorithms as Datalog programs lowered to SQLite

Status: lab, 2026-08-20. Dies on landing per the labs protocol; survivors move to
`sprefa-store/js/src/gen/*.gen.ts` and the prolog emitter.

## TOC
- Constraint first
- The question
- Programs
- Encodings the AST forces today
- Oracles
- Metrics and STANDINGS
- Exit criteria
- Prior art

## Constraint first

SQLite `WITH RECURSIVE` forbids aggregate functions, window functions, and `DISTINCT`
inside the recursive select. The v6 evaluator does not use a recursive CTE; it drives
semi-naive rounds from the host (`lowerSql.ts` `RecursiveStratum`: seed, derive per
recursive body position, merge, promote, `expand` until no growth). That loop is where a
monotone aggregate would live. Today both evaluators refuse it:
`lowerSql.ts:440 AggregateInRecursionError`, `lower.ts:167 recursiveBackend -> "defer"`.

## The question

Which graph algorithms lower to the existing AST (`src/lower/ast.ts`: rels, rules,
`cmp(var, literal)`, stratified negation, post-recursion `max|min|sum|count`) without
engine changes, what each costs in rows and statements, and which need one of:

| missing primitive | algorithms blocked on it |
|---|---|
| monotone aggregate inside recursion (`min`/`max` head in a recursive stratum, merge keeps the better row) | weighted shortest path, tiers and components without the set blow-up |
| arithmetic in the body (`d1 = d + 1`, `c = a + w`) | depth and cost counting without a `succ` table |
| variable-to-variable comparison (`a < b`) | triangle dedup, ordered pairs |
| bounded iteration (`iterate(rounds)`) | PageRank, HITS, label propagation |
| non-monotone loop (delete until stable) | k-core peeling |
| host escape over a `CompactGraph` snapshot | matching, coloring, isomorphism, Louvain |

## Programs

| program | rules (dl reading) | lowers today | note |
|---|---|---|---|
| reach | `reach(x) <- root(x). reach(y) <- reach(x), edge(x, y).` | yes, shipped (`gen/reach.gen.ts`) | baseline |
| level / tiers | `level(x, 0) <- root(x). level(y, d1) <- level(x, d), edge(x, y), succ(d, d1).` then `tier(x, max(d)) <- level(x, d).` | yes | `succ` is an EDB table `0..N`; longest-path tiers on a DAG; on cycles `succ` bounds termination at N |
| components | `label(x, x) <- node(x). label(y, l) <- label(x, l), link(x, y).` then `component(x, min(l)) <- label(x, l).` | yes | `link` = edge plus reverse edge; materialises every (node, label) pair, O(n · component size) |
| unweighted shortest path from sources | `hop(s, s, 0) <- source(s). hop(s, y, d1) <- hop(s, x, d), edge(x, y), succ(d, d1).` then `distance(s, y, min(d)) <- hop(s, y, d).` | yes | every (source, node, depth) triple up to N; the monotone-aggregate version keeps one row per (s, y) |
| triangles | `triangle(a, b, c) <- edge(a, b), edge(b, c), edge(c, a).` then `triangles(count(a)) <- triangle(a, b, c).` | yes | count / 6 on undirected input without `a < b < c` |
| degree | `degree(x, count(y)) <- edge(x, y).` | yes, shipped test | trivial |
| weighted shortest path | `cost(s, s, 0). cost(s, y, c1) <- cost(s, x, c), wedge(x, y, w), c1 = c + w.` with `min` in recursion | no | needs arithmetic + monotone aggregate |
| k-core | delete nodes with degree < k until stable | no | non-monotone loop |
| PageRank | k rounds of `rank(y, sum(rank(x) / outdeg(x)))` | no | needs iterate + arithmetic |
| Borůvka minimum spanning tree | per component pick min edge, merge components, repeat | no | needs min-in-recursion + loop |

## Encodings the AST forces today

- Counting: `succ(d, d1)` EDB rows `(0,1) (1,2) ... (N-1, N)`. Every depth-bearing program
  joins it once per round. It is the standard finite-Datalog trick and it is also the
  termination bound on cyclic inputs.
- Undirected: `link(x, y)` is `edge` unioned with its reverse, as two EDB inserts.
- Aggregates: always in the stratum after the recursion, over the full materialised set.
  The lab measures that set size; it is the argument for monotone aggregates.

## Oracles

Plain TypeScript in the test file, spelled out: breadth-first search for levels and
distances, union-find for components, a triple loop for triangles. No graph library, so the
oracle has no shared code with the thing under test.

## Metrics and STANDINGS

Per program per fixture: derived rows, rounds, statements issued, wall ms for the SQL
evaluator. Fixtures: chain(n), grid(w x h), two-component grid. Written to `STANDINGS.md`
by the bench in the same test file (`GRAPH_LOWERING_BENCH=1`).

## Exit criteria

1. tiers, components, distance, triangles pass the oracle through `evalProgramSql`.
2. STANDINGS shows the row blow-up of set-encoded depth/label against n, which sizes the
   monotone-aggregate change.
3. A written decision on the merge-keeps-better change to `RecursiveStratum.mergeStatement`
   and the `succ`-free arithmetic form, or a written reason to keep the encodings.

## Prior art

SQL/PGQ (SQL:2023 part 16) and DuckPGQ for pattern syntax over relational tables; Soufflé
and LogicBlox for stratified Datalog with aggregates; BigDatalog / Zaniolo's monotonic
aggregates (`mmin`, `mcount`) for exactly the min-in-recursion semantics this lab needs;
Pearce-Kelly for incremental topological order if tiers ever go incremental.

## Findings, first run (2026-08-20)

Test: `sprefa-store/js/tests/labs/graph_lowering.test.ts`, 6/6 pass. Bench:
`just graph-lowering-bench`, numbers in `STANDINGS.md`.

| finding | evidence | consequence |
|---|---|---|
| four of five programs lower today with no engine change | reach, tiers, components, distance, triangles pass their oracles | the AST is enough for the monotone-set half of the table |
| `min` outside the recursion costs nodes x component-size rows | components: chain-200 40,000 rows / 353 ms; chain-1000 1,000,000 rows / 44,047 ms; grid-32x32 1,048,576 rows / 5,001 ms | 10-second-law defect; the merge-keeps-better change to `RecursiveStratum.mergeStatement` is the fix, sized at one statement shape |
| tiers and distance did not blow up on these fixtures | level and hop rows equal node count on chain and on the right/down grid | because every path to a node in those fixtures has one length; a fixture with unequal path lengths (add diagonals) will show the same set growth, add it before sizing |
| statements scale with rounds, not rows | chain-1000: 6007 statements for 999 rounds; grid-32x32: 385 for 62 rounds | semi-naive per-round cost is the floor; rounds equal the graph's depth |
| triangles are four statements | one join stratum, one count stratum | SQL's home turf; no engine work |
| harness DDL uses the `PRIMARY KEY (cols) WITHOUT ROWID` shape | copied from `tests/lower/lowerSql.test.ts` for colocated consistency | the user decision names `("__id" INTEGER PRIMARY KEY, cols, UNIQUE (cols))` as the one shape; the evaluator's `createLikeStatements` is what has to accept it, so that change is the evaluator's, not this lab's |

Last-copy location when this lab dies: the test file moves to `tests/lower/` as the
aggregate-in-recursion battery, the programs to `src/gen/` through the prolog emitter,
STANDINGS rows to `BENCHMARKS.md`.
