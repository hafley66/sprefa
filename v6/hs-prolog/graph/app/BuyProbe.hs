module Main (main) where

import           Prelude
import           Control.Exception (try, evaluate, SomeException)
import qualified Data.Graph as DG
import qualified Data.Graph.Inductive.Graph as F
import qualified Data.Graph.Inductive.PatriciaTree as F

import qualified Data.Graph.Inductive.Query.TransClos as F
import qualified Algebra.Graph.AdjacencyMap as AM
import qualified Algebra.Graph.AdjacencyMap.Algorithm as AMA

main :: IO ()
main = do
  putStrLn "=== containers Data.Graph ==="
  let chain = DG.buildG (1,3) [(1,2),(2,3)]
      cyc   = DG.buildG (1,3) [(1,2),(2,3),(3,1)]
  putStrLn $ "reachable chain from 1 (reflexive?): " ++ show (DG.reachable chain 1)
  putStrLn $ "reachable cyc from 1: " ++ show (DG.reachable cyc 1)
  r1 <- try (evaluate (DG.topSort chain)) :: IO (Either SomeException [DG.Vertex])
  putStrLn $ "topSort acyclic chain: " ++ showEither r1
  r2 <- try (evaluate (DG.topSort cyc)) :: IO (Either SomeException [DG.Vertex])
  putStrLn $ "topSort cyclic: " ++ showEither r2
  let scc = DG.stronglyConnComp [(1,1,[2]),(2,2,[3]),(3,3,[1])]
  putStrLn $ "stronglyConnComp cyclic: " ++ show (map flatten scc)
  putStrLn $ "transposeG chain: " ++ show (DG.transposeG chain)

  putStrLn "=== fgl ==="
  let fg :: F.Gr () ()
      fg = F.mkGraph [(1,()),(2,()),(3,())] [(1,2,()),(2,3,()),(3,1,())]
      fc :: F.Gr () ()
      fc = F.mkGraph [(1,()),(2,()),(3,())] [(1,2,()),(2,3,())]
  putStrLn $ "fgl trc cyc (edges, reflexive?): " ++ show (F.labEdges (F.trc fg))

  putStrLn "=== algebraic-graphs ==="
  let ac = AM.edges [(1,2),(2,3)]
      ay = AM.edges [(1,2),(2,3),(3,1)]
  putStrLn $ "algebraic closure cyc (reflexive?): " ++ show (AM.edgeList (AM.closure ay))
  putStrLn $ "algebraic topSort acyclic: " ++ show (AMA.topSort ac)
  putStrLn $ "algebraic topSort cyclic (Maybe): " ++ show (AMA.topSort ay)
  putStrLn $ "algebraic scc acyclic: " ++ show (AM.edgeList (AMA.scc ac))
  putStrLn $ "algebraic scc cyclic: " ++ show (AM.edgeList (AMA.scc ay))
  putStrLn $ "algebraic transitiveClosure chain edges: " ++ show (AM.edgeList (AM.transitiveClosure ac))

flatten :: DG.SCC a -> [a]
flatten (DG.AcyclicSCC v) = [v]
flatten (DG.CyclicSCC vs) = vs

showEither :: Show a => Either SomeException a -> String
showEither (Right v) = show v
showEither (Left e)  = "THROWS: " ++ show e
