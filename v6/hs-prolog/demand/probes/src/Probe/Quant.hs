-- | Probe: findall/3, bagof/3, setof/3, forall/2 against the Haskell answer.
-- SWI receipts are in DEMAND.md. LogicT (`logict`) is the enumeration.
module Probe.Quant where

import Control.Monad (MonadPlus, mplus, msum)
import Control.Applicative (empty)
import Control.Monad.Logic
import qualified Control.Monad.Logic as L
import Data.List (nub, sort)

-- a relation: likes/2
likes :: MonadPlus m => String -> m String
likes "ann" = msum [return "a", return "b"]
likes "ben" = msum [return "a", return "c"]
likes _     = empty

-- findall/3: collect all answers, in order, with duplicates.
-- Prolog: findall(X, likes(Who, X), Xs)
findAll :: String -> [String]
findAll who = L.observeAll (likes who)

-- setof/3: findall + sort + nub.
setOf :: String -> [String]
setOf who = sort (nub (findAll who))

-- bagof/3: union of a generator over Who, each Who branch separately blocked.
bagOf :: [String]
bagOf = L.observeAll (likes "ann" `mplus` likes "ben")

-- forall/2: no solution violates the body.
-- Prolog: forall(member(X, Ls), predicate(X))
forAll :: [Int] -> (Int -> Bool) -> Bool
forAll xs p = null (filter (not . p) xs)

-- between/3: enumerate a range.
betweenR :: Int -> Int -> Logic Int
betweenR lo hi
  | lo > hi   = empty
  | otherwise = return lo `mplus` betweenR (lo + 1) hi

runQuantProbes :: IO ()
runQuantProbes = do
  let c1 = findAll "ann" == ["a", "b"]
  let c2 = setOf "ben" == ["a", "c"]
  let c3 = sort bagOf == ["a", "a", "b", "c"]
  let c4 = (not (forAll [1, 2, 3] odd)) && (forAll [2, 4, 6] even)
  let c5 = observeAll (betweenR 1 5) == [1, 2, 3, 4, 5]
  putStrLn $ if c1 then "PASS findall" else "FAIL findall"
  putStrLn $ if c2 then "PASS setof" else "FAIL setof"
  putStrLn $ if c3 then "PASS bagof" else "FAIL bagof"
  putStrLn $ if c4 then "PASS forall" else "FAIL forall"
  putStrLn $ if c5 then "PASS between" else "FAIL between"
