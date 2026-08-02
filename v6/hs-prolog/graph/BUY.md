# BUY: which Haskell candidate supplies the five graph jobs

The five jobs, restated from `v6/prolog/0_graph.pl`:

| job | SWI answer |
|---|---|
| strict transitive closure | hand (`library(ugraphs)` transitive_closure/2, strict) |
| topsort that FAILS on cycles | hand (top_sort/2 is the cycle detector) |
| SCC | hand-written Kosaraju (SWI ships none) |
| transpose | hand (transpose_ugraph/2) |
| sparse neighbours | hand (ugraph assoc lookup) |

Semantic facts below were established by RUNNING probe code, not by reading docs:
see `app/BuyProbe.hs` and its output. Traps per the contract: (1) is the
library's transitive closure reflexive or strict, (2) does its topSort fail,
error, or silently return on a cyclic graph. The repo needs STRICT closure and
topsort that FAILS on a cycle.

## Containers Data.Graph

| job | function | measured |
|---|---|---|
| SCC | `stronglyConnComp :: Ord key => [(node,key,[key])] -> [SCC node]` | yes, Tarjan, clean partition. Probe on 1-2-3-1 gives `[[1,2,3]]` |
| topsort | `topSort :: Graph -> [Vertex]` | **wrong framing**: on cycle 1-2-3-1 it SILENTLY returns `[1,2,3]` (not a valid order). No failure, no error. Cannot be the cycle detector |
| strict transitive closure | absent | none exported; `reachable` is REFLEXIVE: chain 1-2-3 from 1 returns `[1,2,3]` including self |
| transpose | `transposeG :: Graph -> Graph` | yes; chain transpose gives 1-[] 2-[1] 3-[2] |
| sparsity | vertex-indexed Int array of edge lists (`buildG`) | sparse: adjacency is a per-vertex list |

## FGL Data.Graph.Inductive

| job | function | measured |
|---|---|---|
| SCC | **absent**: `Data.Graph.Inductive.Query.SCC` is not in the 5.8.3.1 exposed-modules list | cannot import |
| topsort | **absent**: `Data.Graph.Inductive.Query.TopSort` not exposed | cannot import |
| strict transitive closure | `trc` (TransClos) is REFLEXIVE | probe on 1-2-3-1 gives (1,1),(2,2),(3,3) too |
| transpose | `grev :: Graph gr => gr a b -> gr a b` | yes |
| sparsity | `mkGraph`, `neighbors`/`lsuc` | sparse |

FGL ships SCC and TopSort algorithms in source but the released package does not
expose those modules, so as an ecosystem answer they do not exist for an import.

## Algebraic-graphs

| job | function | measured |
|---|---|---|
| SCC | `Algebra.Graph.AdjacencyMap.Algorithm.scc :: Ord a => AdjacencyMap a -> AdjacencyMap a` | returns a CONDENSATION graph (edges between SCCs), not an ordered partition. acyclic gives `[(1,2),(2,3)]`, a 3-cycle gives `[]`. Extracting "each component sorted, list sorted by smallest member" is manual |
| topsort | `Algorithm.topSort :: Ord a => AdjacencyMap a -> Either (NonEmpty a) [a]` | **fails properly**: `Right [1,2,3]` on DAG, `Left (3 :| [1,2])` (the offending cycle) on cyclic. This is the only candidate whose topSort fails on a cycle |
| strict transitive closure | `AdjacencyMap.transitiveClosure` | yes, STRICT: chain 1-2-3 gives `[(1,2),(1,3),(2,3)]`, no self loops |
| transpose | `AdjacencyMap.transpose` | yes |
| sparsity | `AdjacencyMap` is a Map a (Set a) sparse relation | sparse |

## Hand-written over Data.Map

| job | function | measured |
|---|---|---|
| SCC | hand Kosaraju or Tarjan | possible, but this is exactly the failure mode the no-cheat rule names |
| topsort | hand Kahn with leftover detection | possible |
| strict transitive closure | hand DFS/Warshall | possible |
| transpose | hand reverse | possible |
| sparsity | `Map node [node]` | sparse |

## Verdict

Build on **containers `Data.Graph`**. It is the only candidate that supplies the
job SWI lacked, SCC, as an int, tested, ecosystem Tarjan with a clean partition
(`stronglyConnComp`), and it pairs that with vertex-indexed sparse adjacency and
a `transposeG`. Its two gaps are the two jobs it gets wrong on purpose: its
`topSort` silently returns a non-order on a cycle (measured) so it cannot be the
cycle detector, and it exports no transitive closure at all, its `reachable`
being reflexive (measured). So the buy is: lift SCC from `stronglyConnComp`, lift
transpose from `transposeG` / a map transpose, and hand-write only the strict
closure (a per-vertex DFS over the sparse adjacency that naturally excludes a
self reach unless on a cycle) and a Kahn topsort that returns `Nothing` when
in-degree never hits zero. FGL is out because it exposes neither SCC nor
topSort; algebraic-graphs is out because its SCC is a condensation graph rather
than an ordered vertex partition and its `scc`/`topSort` live in a non-obvious
Algorithm module, while it contributes nothing containers already does not for
this module's five explicit outputs. No single library supplies all five with the
required semantics, so the honest ecosystem answer is containers plus two small
hand-written fill-ins that the ecosystem does not provide correctly.
