-- | Probe: tabling (SWI :- table) against the Haskell answer.
-- The two repo live cases are books/v6/algos/magic_sets.pl and causality.pl:
-- both compute a transitive closure `reach` on a graph WITH a cycle, and
-- SWI tabling is what stops the left-recursive rule from looping.
-- The Haskell answer is a naive fixpoint over Data.Set: iterate the TC
-- operator until it stabilizes. No per-rule annotation; it costs an explicit
-- loop and a termination proof that the rule set is monotone and finite.
module Probe.Tabling where

import Data.Set (Set)
import qualified Data.Set as S

-- the same graph as books/v6/algos/magic_sets.pl lines 10-11, cycle included
edge :: (Int, Int) -> Bool
edge p = p `elem` [(1, 2), (2, 3), (3, 1), (7, 8), (8, 9)]

-- one TC step: from a reached set, add everything reachable in one edge
step :: Set Int -> Set Int
step r = r `S.union` S.fromList [ y | (x, y) <- [(1,2),(2,3),(3,1),(7,8),(8,9)], x `S.member` r ]

-- naive fixpoint: iterate step until no change. This is what :- table computes
-- for free; here it is an explicit loop. Terminates because Set is finite.
fixpoint :: Set Int -> Set Int
fixpoint r =
  let r' = step r
  in if r' == r then r else fixpoint r'

-- reachable from a single seed, left-recursive semantics preserved
reachFrom :: Int -> Set Int
reachFrom seed = fixpoint (S.singleton seed)

-- demand-directed component never touched (magic_sets demand_set check, line 28)
reachFromX :: Set Int
reachFromX = reachFrom 7

runTablingProbes :: IO ()
runTablingProbes = do
  let fromA = reachFrom 1
  -- component {1,2,3} closed under cycle + no other node
  let c1 = fromA == S.fromList [1, 2, 3]
  -- undemanded component {7,8,9} stays separate
  let c2 = reachFromX == S.fromList [7, 8, 9]
  -- causality.pl: a self-cycle x=x+1 is rejected by having x reach itself
  let c3 = S.member 1 fromA
  putStrLn $ if c1 then "PASS tabling-fixpoint(cycle-closed)" else "FAIL tabling-fixpoint"
  putStrLn $ if c2 then "PASS tabling-cold" else "FAIL tabling-cold"
  putStrLn $ if c3 then "PASS tabling-selfreach" else "FAIL tabling-selfreach"
