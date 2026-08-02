-- | Probe: assoc, ordsets, pairs against the Haskell answer (containers).
-- Repo receipts: library(assoc) at v6/prolog/0_graph.pl:18,
-- library(ordsets) 4 uses, library(pairs) 7 uses.
module Probe.Collections where

import Data.Map (Map)
import qualified Data.Map as M
import Data.Set (Set)
import qualified Data.Set as S

-- library(pairs): keysort/2, pairs_keys_values/3, map_list_to_pairs/3
-- Haskell answer: List of (k, v); sortBy on the key; unzip.
keysort :: Ord a => [(a, b)] -> [(a, b)]
keysort = sortPairs
  where sortPairs = foldr insertKey []
        insertKey kv acc = merge kv acc
        merge kv [] = [kv]
        merge kv@(k, _) (kv'@(k', _) : rest)
          | k <= k'   = kv : kv' : rest
          | otherwise = kv' : merge kv rest

-- library(assoc): an immutable Map from key to value.
-- list_to_assoc at 0_graph.pl:130, get_assoc at 0_graph.pl:133.
lookupAssoc :: Ord k => k -> Map k v -> Maybe v
lookupAssoc = M.lookup

-- library(ordsets): an ordered Set, union/intersection/ord_subtract.
-- union of two sets; SWI ord_union/3
setUnion :: Ord a => Set a -> Set a -> Set a
setUnion = S.union

runCollectionsProbes :: IO ()
runCollectionsProbes = do
  let pairs :: [(String, Int)]
      pairs = [("b", 2), ("a", 1), ("c", 3)]
  let c1 = keysort pairs == [("a", 1), ("b", 2), ("c", 3)]
  let assoc = M.fromList pairs
  let c2 = lookupAssoc "b" assoc == Just 2 && M.lookup "z" assoc == Nothing
  let c3 :: Bool
      c3 = setUnion (S.fromList [1, 2, 3]) (S.fromList [3, 4]) == S.fromList ([1, 2, 3, 4] :: [Int])
  putStrLn $ if c1 then "PASS pairs-keysort" else "FAIL pairs"
  putStrLn $ if c2 then "PASS assoc" else "FAIL assoc"
  putStrLn $ if c3 then "PASS ordsets" else "FAIL ordsets"
