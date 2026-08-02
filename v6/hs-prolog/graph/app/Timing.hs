module Main (main) where

import           Prelude
import           Control.DeepSeq (NFData, force)
import           Control.Exception (evaluate)
import           System.CPUTime (getCPUTime)
import           Text.Printf (printf)

import           Graph

chainEdges :: Int -> [(Int, Int)]
chainEdges n = [ (i, i + 1) | i <- [1 .. n - 1] ]

time :: NFData a => IO a -> IO (Double, a)
time act = do
  t0 <- getCPUTime
  r <- act
  _ <- evaluate (force r)
  t1 <- getCPUTime
  pure (fromIntegral (t1 - t0) / 1e12, r)

report :: String -> Double -> String
report label d = label ++ printf "%8.3f" d ++ " ms"

main :: IO ()
main = do
  let n = 1000
      g = graphFromEdges (chainEdges n)
  (dScc, comps) <- time (pure (length (graphComponents g)))
  (dClos, clos) <- time (pure (sum (map (length . snd) (graphClosure g))))
  (dSccPairs, sccPairs) <- time (pure (sum (map length (graphComponents g))))
  (dCyclic, cyc) <- time (pure (length (graphCyclicComponents g)))
  putStrLn "1000-node CHAIN"
  putStrLn ("  nodes = " ++ show (length (graphNodes g))
            ++ ", edges = " ++ show (n - 1))
  putStrLn (report "  SCC (containers stronglyConnComp, forced)" dScc
               ++ "  components = " ++ show comps)
  putStrLn (report "  strict closure (sparse DFS, forced)" dClos
               ++ "  total reachable pairs = " ++ show clos)
  putStrLn (report "  cyclic components (forced)" dCyclic
               ++ "  count = " ++ show cyc)
  putStrLn ("  graphHasCycle = " ++ show (graphHasCycle g)
            ++ ", topo = " ++ show (fmap length (graphTopologicalOrder g)))
  putStrLn ""
  putStrLn "SWI reference (v6/prolog plans):"
  putStrLn "  SCC (Kosaraju)                = 27 ms"
  putStrLn "  closure (Warshall composure)  = 27082 ms"
