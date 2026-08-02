-- SWI ground truth for the 11 fixture shapes, captured from
-- `swipl` runs of v6/prolog/0_graph.pl (15/15 plunit pass, plus per-shape
-- answer dumps). This is the differential oracle the grader compares against.
module Golden where

import           Data.List (sort)

-- navigation helpers for building reach/component-of maps from the SWI closure
-- and components (SWI answers include a component-of table per node).
type Nodes = [String]
type Closure = [(String, [String])]
type Comps = [[String]]

data Golden = Golden
  { goldenNodes       :: Nodes
  , goldenClosure     :: Closure
  , goldenReaches     :: [(String, String)]
  , goldenComponents  :: Comps
  , goldenCyclic      :: Comps
  , goldenComponentOf :: [(String, [String])]
  , goldenTopo        :: Maybe Nodes
  , goldenHasCycle    :: Bool
  }

golden :: [(String, Golden)]
golden =
  [ ("empty",                       g [] [] [] [] [] [] (Just []) False)
  , ("single_node",                 g ["lonely"] [("lonely", [])] [] [["lonely"]] [] [("lonely", ["lonely"])] (Just ["lonely"]) False)
  , ("chain",                       g nc ncl ncr n4 [] nco (Just ["a","b","c","d"]) False)
  , ("self_loop",                   g ["a"] [("a",["a"])] [("a","a")] [["a"]] [["a"]] [("a",["a"])] Nothing True)
  , ("mutual_pair",                 g ["a","b"] mpcl mpr [["a","b"]] [["a","b"]] mpco Nothing True)
  , ("three_cycle",                 g ["a","b","c"] tccl tcr [["a","b","c"]] [["a","b","c"]] tcco Nothing True)
  , ("diamond",                     g nc dcl dcr n4 [] dco (Just ["a","b","c","d"]) False)
  , ("two_cycles_joined",           g ["a","b","c","d"] twcl twr [["a","b"],["c","d"]] [["a","b"],["c","d"]] twco Nothing True)
  , ("cycle_with_tail",             g ["a","b","c","d"] cwcl cwr [["a"],["b","c"],["d"]] [["b","c"]] cwco Nothing True)
  , ("flagship_shaped",             g ["mid","reach","sink","src"] flcl flr [["mid"],["reach"],["sink"],["src"]] [["reach"]] flco Nothing True)
  , ("disconnected",                g ["a","b","c","d"] disccl discr [["a"],["b"],["c","d"]] [["c","d"]] discco Nothing True)
  ]
  where
    g = Golden
    -- the four-node DAG shapes share nodes
    nc = ["a","b","c","d"]
    n4 = [["a"],["b"],["c"],["d"]]
    nco = [("a",["a"]),("b",["b"]),("c",["c"]),("d",["d"])]

    -- chain
    ncl = [("a",["b","c","d"]),("b",["c","d"]),("c",["d"]),("d",[])]
    ncr = [("a","b"),("a","c"),("a","d"),("b","c"),("b","d"),("c","d")]

    -- mutual pair
    mpcl = [("a",["a","b"]),("b",["a","b"])]
    mpr = [("a","a"),("a","b"),("b","a"),("b","b")]
    mpco = [("a",["a","b"]),("b",["a","b"])]

    -- three cycle
    tccl = [("a",["a","b","c"]),("b",["a","b","c"]),("c",["a","b","c"])]
    tcr = [("a","a"),("a","b"),("a","c"),("b","a"),("b","b"),("b","c"),("c","a"),("c","b"),("c","c")]
    tcco = [("a",["a","b","c"]),("b",["a","b","c"]),("c",["a","b","c"])]

    -- diamond
    dcl = [("a",["b","c","d"]),("b",["d"]),("c",["d"]),("d",[])]
    dcr = [("a","b"),("a","c"),("a","d"),("b","d"),("c","d")]
    dco = [("a",["a"]),("b",["b"]),("c",["c"]),("d",["d"])]

    -- two cycles joined
    twcl = [("a",["a","b","c","d"]),("b",["a","b","c","d"]),("c",["c","d"]),("d",["c","d"])]
    twr = [("a","a"),("a","b"),("a","c"),("a","d"),("b","a"),("b","b"),("b","c"),("b","d"),("c","c"),("c","d"),("d","c"),("d","d")]
    twco = [("a",["a","b"]),("b",["a","b"]),("c",["c","d"]),("d",["c","d"])]

    -- cycle with tail
    cwcl = [("a",["b","c","d"]),("b",["b","c","d"]),("c",["b","c","d"]),("d",[])]
    cwr = [("a","b"),("a","c"),("a","d"),("b","b"),("b","c"),("b","d"),("c","b"),("c","c"),("c","d")]
    cwco = [("a",["a"]),("b",["b","c"]),("c",["b","c"]),("d",["d"])]

    -- flagship shaped
    flcl = [("mid",["reach","sink"]),("reach",["reach","sink"]),("sink",[]),("src",["mid","reach","sink"])]
    flr = [("mid","reach"),("mid","sink"),("reach","reach"),("reach","sink"),("src","mid"),("src","reach"),("src","sink")]
    flco = [("mid",["mid"]),("reach",["reach"]),("sink",["sink"]),("src",["src"])]

    -- disconnected
    disccl = [("a",["b"]),("b",[]),("c",["c","d"]),("d",["c","d"])]
    discr = [("a","b"),("c","c"),("c","d"),("d","c"),("d","d")]
    discco = [("a",["a"]),("b",["b"]),("c",["c","d"]),("d",["c","d"])]

-- edges per shape, matching 0_graph.test.pl
shapeEdgesFor :: String -> [(String, String)]
shapeEdgesFor s = case s of
  "empty"              -> []
  "single_node"        -> []
  "chain"              -> [("a","b"),("b","c"),("c","d")]
  "self_loop"          -> [("a","a")]
  "mutual_pair"        -> [("a","b"),("b","a")]
  "three_cycle"        -> [("a","b"),("b","c"),("c","a")]
  "diamond"            -> [("a","b"),("a","c"),("b","d"),("c","d")]
  "two_cycles_joined"  -> [("a","b"),("b","a"),("b","c"),("c","d"),("d","c")]
  "cycle_with_tail"    -> [("a","b"),("b","c"),("c","b"),("c","d")]
  "flagship_shaped"    -> [("src","mid"),("mid","reach"),("reach","reach"),("reach","sink")]
  "disconnected"       -> [("a","b"),("c","d"),("d","c")]
  _                    -> []

-- sortNormalize sorts a list of pairs and dedups, used for comparing
-- reach/component-of tables independent of enumeration order.
sortN :: Ord a => [a] -> [a]
sortN = sort
