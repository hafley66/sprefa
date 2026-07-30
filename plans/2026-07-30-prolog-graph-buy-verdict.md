# Prolog graph operations: buy-before-build analysis

Date: 2026-07-30
Base: `7d040ab062962173556ef44c7e0d4bb6bb90488b`

Written before any implementation, per the standing law. Every number below
was measured on this machine, not recalled.

## The problem being solved

`plans/2026-07-30-prolog-compile-profiling.md` measured the compiler's `plan`
phase at 255,333 ms of 255,490 ms on `flagship-flow.dl6`, at 6,011,087,004
inferences, with `clock_check:graph_reachable/4` under `clock_scc/3` named as
the hot predicate.

The dependency graph that cost 255 seconds:

```text
v6/dl/fixtures/flagship-flow.dl6: decls=215 rules=36 deps=64
                                  causal_edges=60 nodes=42
```

42 nodes. 60 edges. The algorithm, not the data, is the cost.

Two modules carry independent copies of the same algorithm, all-pairs mutual
reachability where each reachability call enumerates simple paths with a
`Visited` list and no memo:

| site | predicates |
|---|---|
| `v6/prolog/compile/3_clock_check.pl:224-234` | `graph_reachable/3,4`, `clock_scc/3` |
| `v6/prolog/labs/rel_definition_hash/0_receipts.pl:296-315` | `reachable/3,4`, `relation_scc/3`, `mutually_reachable/3` |

## Operations actually needed

Surveyed across `v6/prolog/`: reachability, transitive closure, strongly
connected components, topological order, cycle detection.

## Candidates

### 1. `library(ugraphs)`

Ships with SWI 10.0.2. Zero prior uses in this repo (verified by grep over
`v6/` and `src/`). Exports measured directly from the installed library:

```text
add_edges/3      add_vertices/3   complement/2      compose/3
del_edges/3      del_vertices/3   edges/2           neighbors/3
neighbours/3     reachable/3      top_sort/2        ugraph_layers/2
transitive_closure/2              transpose_ugraph/2
vertices/2       vertices_edges_to_ugraph/3         ugraph_union/3
connect_ugraph/3
```

**There is no strongly-connected-component predicate in that list.** The only
file under the SWI library tree mentioning SCC at all is
`library/clp/clpfd.pl`, where it is internal to `circuit/1` propagation and
not exported.

Semantics verified by running them rather than by reading the docs, because
the checker being replaced depends on the exact strictness:

```text
transitive_closure([1-[2],2-[3],3-[]], C)  ->  [1-[2,3],2-[3],3-[]]
transitive_closure([1-[2],2-[1]],      C)  ->  [1-[1,2],2-[1,2]]
transitive_closure([1-[1]],            C)  ->  [1-[1]]
transitive_closure([1-[]],             C)  ->  [1-[]]
top_sort([1-[2],2-[1]], X)                 ->  FAIL
top_sort([1-[1]],       X)                 ->  FAIL
reachable(1, [1-[]], R)                    ->  [1]
```

Two findings that decide the wiring. `transitive_closure/2` is the STRICT
positive-length closure, which is exactly what the shipped `graph_reachable/3`
means, so it is a semantics-preserving swap. `reachable/3` is REFLEXIVE and is
therefore **not** a drop-in for `graph_reachable/3`; using it would silently
have made every acyclic node its own component.

| gives | costs |
|---|---|
| representation, `transpose_ugraph/2`, `neighbours/3` | nothing, all linear |
| `top_sort/2` = topological order AND cycle detection (fails on cycles, self-loops included) | nothing, Kahn's algorithm |
| `transitive_closure/2` = reachability | Warshall, materializes the full O(V^2) relation |
| strongly connected components | **not shipped** |

Handles the shapes this compiler produces: yes for cyclic rule graphs,
self-loops and multi-head recursive strata, all confirmed by the shape table
below.

### 2. SWI tabling (`:- table`) applied to the existing predicates

`:- table` appears twice in the tree today (`v6/prolog/src/kernel.pl:9`,
`v6/sprefa-store/bench/swi_reach.pl:23`) and in none of the type, clock or
inference walkers. Built in, zero dependencies.

Tabling the existing `reachable/2` shape turns simple-path enumeration into a
memoized least fixpoint, which is a real and correct fix.

Cost: the natural spelling tables over an asserted `edge/2` relation, so the
graph has to reach the database. Tabling the predicates as they stand instead
means the whole `Dependencies` list, and through it the whole `Program` term,
becomes part of the variant key, so every call pays a hash of the program.

### 3. Hand-written Tarjan or Kosaraju

Bespoke. Linear in vertices plus edges.

### 4. Published SWI packs

Queried the pack server. `pack_list('graph', ...)` returns
`callgraph`, `disp_bn`, `egraph`, `graphml`, `graphpl`, `graphql`,
`graphql-swipl`, `gvterm`, `logicmoo_cg`, `musicbrainz`, `plcairo`,
`prolog_graphviz`, `pub_graph`, `sindice`, `sourcehut`, `wgraph`. Those are
visualisation, serialisation and API clients. `pack_list('scc', ...)` returns
`no matching packages`.

The one arguable fit, `graphpl@0.1.1` "Graph data structure utilities", is a
0.1.x third-party pack. Taking it would add the first pack dependency to a
prolog toolchain that currently has zero, to cover an operation no pack was
found to implement anyway. Rejected on that, not on a one-line dismissal:
the search was run and its output is above.

## Measurements

All four implementations were run against each other on the same shapes in one
process. **Every implementation agreed on every component answer at every
size** (the harness compares and prints a mismatch line; none printed). Times
in ms.

| shape | V | E | current shipped | ugraphs closure | SWI tabling | hand Tarjan |
|---|---:|---:|---:|---:|---:|---:|
| flagship (real) | 42 | 60 | 202 | 2 | 5 | 1 |
| chain58 | 59 | 58 | 868 | 7 | 9 | 1 |
| dense20 | 20 | 60 | **>60,000 (time limit)** | 1 | 2 | 0 |
| dense60 | 60 | 180 | not run | 12 | 13 | 0 |
| chain300 | 301 | 300 | not run | 758 | 276 | 5 |
| dense300 | 300 | 900 | not run | 922 | 343 | 4 |
| sparse500 + 20 cycles | 501 | 520 | not run | 3,176 | 1,681 | 8 |
| chain1000 | 1001 | 1000 | not run | 27,082 | 3,384 | 27 |
| dense1000 | 1000 | 3000 | not run | 28,543 | 4,379 | 22 |

Two things fall out.

The shipped algorithm's cost tracks CONNECTIVITY, not size: a 20-node graph
with 60 edges did not finish inside 60 seconds, while a 59-node chain took
868 ms. That is why a 27-rule real program cost 255 s while a 30-rule chain
cost 3.6 s.

The ugraphs closure composition's cost tracks VERTEX COUNT, not sparsity:
`chain1000` is acyclic with 1000 edges and still took 27,082 ms, because
Warshall materializes all O(V^2) reachable pairs regardless of shape. Using it
as the component engine would remove today's landmine and plant a slower one
at a few hundred rules.

## Verdict

**Buy `library(ugraphs)` for representation, transpose, neighbours,
topological order and cycle detection.** All linear, all shipped, zero
dependencies, and its `transitive_closure/2` is the exact strictness the clock
checker's reachability already meant.

**The one operation no library and no pack ships is the component
decomposition itself.** For that, the choice is between composing it from
`transitive_closure/2` (27,082 ms at 1000 nodes) and a direct search (27 ms).
Kosaraju is taken, written over ugraphs' own `transpose_ugraph/2` and
neighbour structure rather than over a private representation, at about 40
lines.

Writing it is justified here only because the library research above was run
first and came back empty on this specific operation. It is not justified by
familiarity, and the alternative was not dismissed in one line: the Warshall
composition is kept as the differential test oracle in
`v6/prolog/compile/test/0_graph.test.pl`, so the bought spelling still checks
the built one on every shape, forever. Slow and obviously correct is what a
test oracle should be.

Tabling is not taken. It is a real fix and it is built in, but at the sizes
this compiler produces it is measurably slower than both alternatives
(5 ms against 1 ms on the flagship graph), and the spelling that avoids
putting the whole program term into a variant key requires the graph to be
asserted into the database, which is a larger change than the one being made.

## Consequence for the module

`v6/prolog/0_graph.pl`, one module, the operations above, no global state and
no tabling. Both existing call sites move onto it and their private copies are
deleted.
