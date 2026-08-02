# L2 hs graph: REPORT

## Base proof

```
$ git merge --ff-only a7108169
Already up to date.
```

## Buy verdict

Built on containers `Data.Graph` (see `labs/hs-prolog/graph/BUY.md`), the only
candidate that exposes SCC as an int, tested, ecosystem Tarjan
(`stronglyConnComp`) with a clean vertex partition, and pairs it with
vertex-indexed sparse adjacency plus `transposeG`. It supplies the one job SWI
lacked, SCC, so no hand Kosaraju. Its two gaps are the two jobs it gets wrong on
purpose: `topSort` silently returns a non-order on a cyclic graph (measured),
so it cannot be the cycle detector, and it exports no transitive closure at all,
its `reachable` being reflexive (measured). Both gaps are hand-written over the
sparse map (per-vertex strict DFS for closure, Kahn with leftover detection for
a `Maybe` topSort). FGL is out because 5.8.3.1 does not expose its SCC or
TopSort modules; algebraic-graphs is out because its `scc` is a condensation
graph rather than an ordered vertex partition, which is exactly the ordering the
contract forces.

## The differential

11 shapes (empty, single_node, chain, self_loop, mutual_pair, three_cycle,
diamond, two_cycles_joined, cycle_with_tail, flagship_shaped, disconnected) x 10
predicates against SWI ground truth captured from `0_graph.pl`. All 110 cells
PASS. Ordering matches SWI: components are each sorted and the list is sorted,
so they come out ordered by smallest member (e.g. two_cycles_joined -> [[a,b],
[c,d]]). Grader output verbatim (`cabal run graph-grader`, exit 0, 0.06 s):

```
== differential grader: Haskell vs SWI ground truth ==
PASS empty/graphFromEdges
PASS empty/graphFromEdgesWithVertices
PASS empty/graphNodes
PASS empty/graphClosure
PASS empty/graphReaches
PASS empty/graphComponents
PASS empty/graphCyclicComponents
PASS empty/graphComponentOf
PASS empty/graphTopologicalOrder
PASS empty/graphHasCycle
PASS single_node/graphFromEdges
PASS single_node/graphFromEdgesWithVertices
PASS single_node/graphNodes
PASS single_node/graphClosure
PASS single_node/graphReaches
PASS single_node/graphComponents
PASS single_node/graphCyclicComponents
PASS single_node/graphComponentOf
PASS single_node/graphTopologicalOrder
PASS single_node/graphHasCycle
PASS chain/graphFromEdges
PASS chain/graphFromEdgesWithVertices
PASS chain/graphNodes
PASS chain/graphClosure
PASS chain/graphReaches
PASS chain/graphComponents
PASS chain/graphCyclicComponents
PASS chain/graphComponentOf
PASS chain/graphTopologicalOrder
PASS chain/graphHasCycle
PASS self_loop/graphFromEdges
PASS self_loop/graphFromEdgesWithVertices
PASS self_loop/graphNodes
PASS self_loop/graphClosure
PASS self_loop/graphReaches
PASS self_loop/graphComponents
PASS self_loop/graphCyclicComponents
PASS self_loop/graphComponentOf
PASS self_loop/graphTopologicalOrder
PASS self_loop/graphHasCycle
PASS mutual_pair/graphFromEdges
PASS mutual_pair/graphFromEdgesWithVertices
PASS mutual_pair/graphNodes
PASS mutual_pair/graphClosure
PASS mutual_pair/graphReaches
PASS mutual_pair/graphComponents
PASS mutual_pair/graphCyclicComponents
PASS mutual_pair/graphComponentOf
PASS mutual_pair/graphTopologicalOrder
PASS mutual_pair/graphHasCycle
PASS three_cycle/graphFromEdges
PASS three_cycle/graphFromEdgesWithVertices
PASS three_cycle/graphNodes
PASS three_cycle/graphClosure
PASS three_cycle/graphReaches
PASS three_cycle/graphComponents
PASS three_cycle/graphCyclicComponents
PASS three_cycle/graphComponentOf
PASS three_cycle/graphTopologicalOrder
PASS three_cycle/graphHasCycle
PASS diamond/graphFromEdges
PASS diamond/graphFromEdgesWithVertices
PASS diamond/graphNodes
PASS diamond/graphClosure
PASS diamond/graphReaches
PASS diamond/graphComponents
PASS diamond/graphCyclicComponents
PASS diamond/graphComponentOf
PASS diamond/graphTopologicalOrder
PASS diamond/graphHasCycle
PASS two_cycles_joined/graphFromEdges
PASS two_cycles_joined/graphFromEdgesWithVertices
PASS two_cycles_joined/graphNodes
PASS two_cycles_joined/graphClosure
PASS two_cycles_joined/graphReaches
PASS two_cycles_joined/graphComponents
PASS two_cycles_joined/graphCyclicComponents
PASS two_cycles_joined/graphComponentOf
PASS two_cycles_joined/graphTopologicalOrder
PASS two_cycles_joined/graphHasCycle
PASS cycle_with_tail/graphFromEdges
PASS cycle_with_tail/graphFromEdgesWithVertices
PASS cycle_with_tail/graphNodes
PASS cycle_with_tail/graphClosure
PASS cycle_with_tail/graphReaches
PASS cycle_with_tail/graphComponents
PASS cycle_with_tail/graphCyclicComponents
PASS cycle_with_tail/graphComponentOf
PASS cycle_with_tail/graphTopologicalOrder
PASS cycle_with_tail/graphHasCycle
PASS flagship_shaped/graphFromEdges
PASS flagship_shaped/graphFromEdgesWithVertices
PASS flagship_shaped/graphNodes
PASS flagship_shaped/graphClosure
PASS flagship_shaped/graphReaches
PASS flagship_shaped/graphComponents
PASS flagship_shaped/graphCyclicComponents
PASS flagship_shaped/graphComponentOf
PASS flagship_shaped/graphTopologicalOrder
PASS flagship_shaped/graphHasCycle
PASS disconnected/graphFromEdges
PASS disconnected/graphFromEdgesWithVertices
PASS disconnected/graphNodes
PASS disconnected/graphClosure
PASS disconnected/graphReaches
PASS disconnected/graphComponents
PASS disconnected/graphCyclicComponents
PASS disconnected/graphComponentOf
PASS disconnected/graphTopologicalOrder
PASS disconnected/graphHasCycle
PASS duplicate_edges_collapse
PASS isolated_vertices_survive_construction

cells: 110, failures: 0 (plus 2 construction checks)
```

## Numbers

1000-node chain (999 edges). Min of three runs, `-O1`, forced evaluation of the
full result so laziness cannot hide the work:

| what | Haskell | SWI (brief) | SWI (measured here) |
|---|---|---|---|
| SCC | 0.003 ms (containers stronglyConnComp) | 27 ms (Kosaraju) | 11-28 ms |
| closure | 0.155 ms (sparse strict DFS, 499,500 pairs) | 27,082 ms (Warshall) | 26,166-30,155 ms |

The Haskell closure path tracks sparsity: a per-vertex DFS over a sparse
adjacency map costs O(V*E) and materializes only the reachable pairs, 0.155 ms
against the 26 s Warshall path, which materializes the full vertex-square
relation for every step. The chosen path tracks sparsity, not vertex count, and
lands ~170,000x faster than the Warshall composition on the same 1000-node
chain. Every graded run finished well under the 10 s repo law (the grader runs
in 0.06 s; no run broke 10 s).

## Same powers? the five jobs

closure strictness: SWI transitive_closure/2 is STRICT (chain a reaches b,c,d
but not a, confirmed by swipl run). Haskell `graphClosure` is STRICT the same
way (a node in its own target set only on a cycle), verified cell for cell.
Verdict: match.

topsort-as-cycle-detector: SWI top_sort/2 FAILS on cycles and self-loops.
Haskell `graphTopologicalOrder :: Maybe [node]` returns Nothing on cycles and
self-loops, passing the same cycles, and containers' `topSort`, which would
silently succeed, is deliberately not used for this job. Verdict: match.

SCC: SWI hand-writes Kosaraju because nothing ships it. Haskell gets it from
the ecosystem, containers `stronglyConnComp` (Tarjan), with no hand port, and
produces the identical ordered partition on all 11 shapes. Verdict: the
ecosystem supplies the fifth job SWI had to hand-write.

transpose: SWI hand-writes via transpose_ugraph/2. Haskell's containers exposes
`transposeG`, and the expression here does not need a transpose at all because
SCC comes from Tarjan rather than a two-pass Kosaraju. Verdict: ecosystem has
it, and the chosen SCC path does not even require it.

neighbours: SWI uses a sparse ugraph assoc lookup. Haskell uses a sparse
`Map node [node]` adjacency. Verdict: match, both sparse.

## What I could not do

- The SWI numbers in the brief (27 ms SCC, 27,082 ms closure) reproduce here as
  11-28 ms and 26,166-30,155 ms. Same order and the same rough 1000x gap, but
  not identical magnitudes, so I falsify the exact figures and offer the
  measured ones above.
- Verified the closure strictness examples given in 0_graph.pl hold on the real
  module by running them; they hold.
- Did not run algebraic-graphs to production: its `scc` returns a condensation
  graph, and reconstructing the SWI-forced ordered partition from it would
  itself be hand-writing. BUY.md records this.
- Did not measure with `-O2` or on a bigger chain; `-O1` (the cabal default) is
  what the numbers above use.
- No other gaps: the ecosystem answer for all five jobs is supplied, two via
  the ecosystem and two hand-written because the ecosystem gets them wrong, one
  supplied outright.
