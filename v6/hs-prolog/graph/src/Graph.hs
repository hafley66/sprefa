-- The ugraph shape is a sorted vertex-to-sorted-neighbours association with
-- unique keys. Answers come out in the order 0_graph.pl produces, so sortedness
-- is part of the contract. SCC is lifted from containers
-- Data.Graph.stronglyConnComp (the ecosystem Tarjan) per BUY.md; closure and
-- cycle-failing topsort are the two jobs containers gets wrong, so they are
-- hand-written over a sparse Map node [node].
module Graph
  ( Graph
  , graphFromEdges
  , graphFromEdgesWithVertices
  , graphNodes
  , graphClosure
  , graphReaches
  , graphComponents
  , graphCyclicComponents
  , graphComponentOf
  , graphTopologicalOrder
  , graphHasCycle
  ) where

import           Prelude hiding (lookup)
import           Data.List (sort, nub, lookup)
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import qualified Data.Graph as DG

-- adjacency: every vertex is a key; isolated vertices map to []
newtype Graph node = Graph { adj :: M.Map node [node] }
  deriving (Eq, Show)

neigh :: Ord node => Graph node -> node -> [node]
neigh g n = M.findWithDefault [] n (adj g)

-- graphFromEdges edges: vertices are the endpoints, duplicate edges collapse.
graphFromEdges :: Ord node => [(node, node)] -> Graph node
graphFromEdges edges =
  let ns = sort (nub (concatMap (\(a, b) -> [a, b]) edges))
  in build ns edges

-- graphFromEdgesWithVertices vertices edges: vertices seeds isolated nodes that
-- no edge mentions; an endpoint absent from vertices is still added.
graphFromEdgesWithVertices :: Ord node => [node] -> [(node, node)] -> Graph node
graphFromEdgesWithVertices vertices edges =
  let ns = sort (nub (vertices ++ concatMap (\(a, b) -> [a, b]) edges))
  in build ns edges

build :: Ord node => [node] -> [(node, node)] -> Graph node
build ns edges =
  let acc = M.fromListWith (++) [ (f, [t]) | (f, t) <- edges ]
      base = M.fromList (zip ns (repeat []))
  in Graph (M.map (sort . nub) (M.unionWith (++) acc base))

graphNodes :: Ord node => Graph node -> [node]
graphNodes = M.keys . adj

-- STRICT reachability: paths of length at least one. A node appears in its own
-- target set exactly when it sits on a cycle. DFS from each node's neighbours,
-- so a node is excluded unless a cycle brings it back through another node.
graphClosure :: Ord node => Graph node -> [(node, [node])]
graphClosure g =
  [ (n, S.toAscList (reachNodes g n)) | n <- graphNodes g ]

reachNodes :: Ord node => Graph node -> node -> S.Set node
reachNodes g start = go S.empty (neigh g start)
  where
    go seen [] = seen
    go seen (x : xs)
      | x `S.member` seen = go seen xs
      | otherwise = go (S.insert x seen) (neigh g x ++ xs)

graphReaches :: Ord node => [(node, [node])] -> node -> node -> Bool
graphReaches closure from to =
  case lookup from closure of
    Nothing -> False
    Just ts -> to `elem` ts

-- Every vertex lands in exactly one component; a vertex on no cycle is its own
-- singleton. Each component sorted, the list sorted, so components come out
-- ordered by smallest member. SCC is the ecosystem Tarjan.
graphComponents :: Ord node => Graph node -> [[node]]
graphComponents g =
  let triples = [ (n, n, neigh g n) | n <- graphNodes g ]
      sccs = map sccToList (DG.stronglyConnComp triples)
  in sort (map sort sccs)

sccToList :: DG.SCC node -> [node]
sccToList (DG.AcyclicSCC n) = [n]
sccToList (DG.CyclicSCC ns) = ns

-- graphComponents restricted to components holding at least one internal edge:
-- every multi-node component, plus singletons carrying a self-loop.
graphCyclicComponents :: Ord node => Graph node -> [[node]]
graphCyclicComponents g =
  filter (hasInternalEdge g) (graphComponents g)

hasInternalEdge :: Ord node => Graph node -> [node] -> Bool
hasInternalEdge _ (_ : _ : _) = True
hasInternalEdge g [n] = n `elem` neigh g n
hasInternalEdge _ [] = False

graphComponentOf :: Ord node => [[node]] -> node -> Maybe [node]
graphComponentOf comps n = foldr pick Nothing comps
  where
    pick c acc = if n `elem` c then Just c else acc

-- Nothing on a cyclic graph, self-loops included. This is also the cycle
-- detector. Kahn with leftover detection: if the zero-in-degree set ever
-- empties while vertices remain, a cycle is present.
graphTopologicalOrder :: Ord node => Graph node -> Maybe [node]
graphTopologicalOrder g = kahn g (indegree g)

indegree :: Ord node => Graph node -> M.Map node Int
indegree g = M.fromList [ (n, countIn n) | n <- graphNodes g ]
  where
    countIn n = length [ v | v <- graphNodes g, n `elem` neigh g v ]

kahn :: Ord node => Graph node -> M.Map node Int -> Maybe [node]
kahn g indeg = go (sort [ n | (n, d) <- M.toList indeg, d == 0 ]) indeg []
  where
    go [] indeg acc
      | M.null indeg = Just (reverse acc)
      | otherwise = Nothing
    go (n : ns) indeg acc =
      let decremented = foldr (\m m0 -> M.adjust (subtract 1) m m0) indeg (neigh g n)
          indeg' = M.delete n decremented
          newZeros = sort [ m | m <- neigh g n
                              , M.lookup m indeg == Just 1 ]
      in go (nubSort (newZeros ++ ns)) indeg' (n : acc)

nubSort :: Ord a => [a] -> [a]
nubSort = sort . nub

graphHasCycle :: Ord node => Graph node -> Bool
graphHasCycle g =
  case graphTopologicalOrder g of
    Nothing -> True
    Just _ -> False
