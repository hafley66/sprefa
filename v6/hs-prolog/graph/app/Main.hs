{-# LANGUAGE TupleSections #-}
module Main (main) where

import           Prelude
import           Data.List (sort)
import           Data.Maybe (fromMaybe)
import           System.Exit (exitFailure)

import           Graph
import qualified Golden as G

allShapes :: [String]
allShapes = map fst G.golden

-- build the graph exactly as the SWI test's shape_graph/2 does
mkGraph :: String -> Graph String
mkGraph name =
  case name of
    "single_node" -> graphFromEdgesWithVertices ["lonely"] []
    _             -> graphFromEdges (G.shapeEdgesFor name)

-- the WithVertices construction: single_node seeds [lonely], everyone else
-- seeds nothing extra, so both constructors must agree on the node set.
mkGraphWithVertices :: String -> Graph String
mkGraphWithVertices name =
  let seed = case name of "single_node" -> ["lonely"]; _ -> []
  in graphFromEdgesWithVertices seed (G.shapeEdgesFor name)

gradeShape :: String -> [(String, String, String, Bool)]
gradeShape name =
  let g = mkGraph name
      go = fromMaybe (error ("no golden for " ++ name)) (lookup name G.golden)
      ck pred detail ok = [(name, pred, detail, ok)]
      nodes = graphNodes g
  in concat
    [ ck "graphFromEdges" (show nodes) (nodes == G.goldenNodes go)
    , ck "graphFromEdgesWithVertices"
        (show (graphNodes (mkGraphWithVertices name)))
        (graphNodes (mkGraphWithVertices name) == G.goldenNodes go)
    , ck "graphNodes" (show nodes) (nodes == G.goldenNodes go)
    , ck "graphClosure" (show (graphClosure g)) (graphClosure g == G.goldenClosure go)
    , ck "graphReaches" (show (sort (allReaches g))) 
        (sort (allReaches g) == sort (G.goldenReaches go))
    , ck "graphComponents" (show (graphComponents g)) (graphComponents g == G.goldenComponents go)
    , ck "graphCyclicComponents" (show (graphCyclicComponents g)) (graphCyclicComponents g == G.goldenCyclic go)
    , ck "graphComponentOf" (show (sort (allComponentOf g)))
        (sort (allComponentOf g) == sort (G.goldenComponentOf go))
    , ck "graphTopologicalOrder" (show (graphTopologicalOrder g))
        (graphTopologicalOrder g == G.goldenTopo go)
    , ck "graphHasCycle" (show (graphHasCycle g)) (graphHasCycle g == G.goldenHasCycle go)
    ]

-- all (from,to) strict-reachable pairs implied by our closure
allReaches :: Graph String -> [(String, String)]
allReaches g = [ (f, t) | (f, ts) <- graphClosure g, t <- ts ]

-- component-of map for every node
allComponentOf :: Graph String -> [(String, [String])]
allComponentOf g =
  [ (n, c) | n <- graphNodes g
           , Just c <- [graphComponentOf (graphComponents g) n] ]

constructionExtras :: [String]
constructionExtras =
  [ if graphFromEdges [("a","b"),("a","b"),("b","a")] == graphFromEdges [("a","b"),("b","a")]
      then "PASS duplicate_edges_collapse"
      else "fail duplicate_edges_collapse"
  , if graphNodes (graphFromEdgesWithVertices ["lonely"] [("a","b")]) == ["a","b","lonely"]
      then "PASS isolated_vertices_survive_construction"
      else "fail isolated_vertices_survive_construction"
  ]

main :: IO ()
main = do
  putStrLn "== differential grader: Haskell vs SWI ground truth =="
  let results = concatMap gradeShape allShapes
      fails = [ r | r@(_,_,_,ok) <- results, not ok ]
  mapM_ printResult results
  mapM_ putStrLn constructionExtras
  putStrLn ""
  putStrLn ("cells: " ++ show (length results)
            ++ ", failures: " ++ show (length fails)
            ++ " (plus 2 construction checks)")
  if null fails then pure () else exitFailure

printResult :: (String, String, String, Bool) -> IO ()
printResult (shape, pred, detail, ok) =
  putStrLn (if ok
              then "PASS " ++ shape ++ "/" ++ pred
              else "fail " ++ shape ++ "/" ++ pred ++ "    got: " ++ detail)
