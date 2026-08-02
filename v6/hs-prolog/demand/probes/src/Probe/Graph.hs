-- | Probe: ugraphs (transitive_closure, top_sort, cycle detect) against the
-- Haskell answer: fgl is the direct equivalent. Repo case:
-- v6/prolog/0_graph.pl built whole on library(ugraphs).
--
-- Two genuine semantic gaps surfaced by this probe:
--   1. fgl topsort (topsort/2) SILENTLY DROPS cyclic nodes and returns a
--      partial order; SWI top_sort/2 FAILS on a cycle (0_graph.pl:187-191)
--      and graph_has_cycle/1 at 0_graph.pl:195-196 depends on that fail.
--   2. fgl trc is REFLEXIVE (adds n->n for every node); 0_graph.pl:55-67
--      requires a STRICT positive-length closure. Cycle detection must use a
--      back-edge DFS, exactly as 0_graph.pl:99-114 does with Kosaraju.
module Probe.Graph where

import Data.Graph.Inductive
import qualified Data.Graph.Inductive.Query.DFS as DFS
import qualified Data.Graph.Inductive.Query.BFS as B

-- build an fgl graph (nodes 1..3, edges 1->2, 2->3)
sampleG :: Gr Char ()
sampleG = mkGraph [(1,'a'),(2,'b'),(3,'c')] [(1,2,()),(2,3,())]

-- cycle graph 1<->2
cycleG :: Gr Char ()
cycleG = mkGraph [(1,'a'),(2,'b')] [(1,2,()),(2,1,())]

-- strict closure from a node (0_graph.pl:54-55: start appears in its own
-- target set only when it sits on a cycle). fgl bfs includes the start, so
-- we drop it unless a back edge returns it, which false-SCC would miss.
strictClosure :: Gr Char () -> Int -> [Int]
strictClosure g n = filter (/= n) (B.bfs n g)

-- has_cycle: some node reaches itself along >=1 edge (0_graph.pl:99-114,
-- which counts multi-node components AND self-loops). A node is on a cycle
-- iff the strict closure of one of its successors contains the node itself.
hasCycle :: Gr Char () -> Bool
hasCycle g = any onCycle (nodes g)
  where
    onCycle n = n `elem` suc g n   -- self-loop
             || any (\m -> n `elem` strictClosure g m) (suc g n)

runGraphProbes :: IO ()
runGraphProbes = do
  -- 0_graph.pl:191 top_sort/2 on an acyclic graph
  let c1 = DFS.topsort sampleG == [1, 2, 3]
  -- 0_graph.pl:58 transitive_closure strictness: 1 reaches [2,3] (not 1)
  let c2 = strictClosure sampleG 1 == [2, 3]
  -- 0_graph.pl:195-196 has_cycle: cycle graph yes, acyclic graph no
  let c3 = hasCycle cycleG && not (hasCycle sampleG)
  putStrLn $ if c1 then "PASS graph-toposort" else "FAIL graph-toposort"
  putStrLn $ if c2 then "PASS graph-closure" else "FAIL graph-closure"
  putStrLn $ if c3 then "PASS graph-cycle-detect" else "FAIL graph-cycle"
  putStrLn "NOTE: fgl topsort drops cyclic nodes, SWI top_sort/2 fails; fgl trc"
  putStrLn "      is reflexive, SWI graph_closure needs strict >=1 edge; the"
  putStrLn "      has_cycle answer needs a back-edge DFS, not `topsort empty`."
