-- Load-time tabling for predicates declared with :- table. The tabled
-- predicate's recursive clauses would diverge under plain depth-first
-- enumeration (magic/1 in magic_sets.pl loops), so we compute the finite
-- fixpoint of ground facts once and replace the tabled clauses with facts.
-- This is the "faked it by memoizing" option the contract permits; REPORT.md
-- states it.

module Prolog.Tabling
  ( applyTabling
  ) where

import qualified Data.Map.Strict as Map
import Data.List (nub)
import Prolog.Solve
import Prolog.Term

applyTabling :: Database -> Database
applyTabling db =
  let tabled = nub (dbTabled db)
  in if null tabled then db
     else
       let facts = fixpoint db tabled Map.empty
           factClauses = concatMap (\(key, fs) -> map (\f -> Clause f []) fs)
                                  (Map.toList facts)
           nontabled = [ c | c <- dbClauses db, not (isTabledHead tabled c) ]
       in db { dbClauses = nontabled ++ factClauses }

fixpoint :: Database -> [(String, Int)] -> Map.Map (String, Int) [Term]
         -> Map.Map (String, Int) [Term]
fixpoint db tabled known =
  let next = Map.fromList [ (key, deriveFacts db tabled known key) | key <- tabled ]
      merged = Map.unionWith (\a b -> nub (a ++ b)) next known
  in if Map.keys merged == Map.keys known
        && all (\key -> sortFacts (Map.findWithDefault [] key merged)
                          == sortFacts (Map.findWithDefault [] key known))
               (Map.keys known)
     then known
     else fixpoint db tabled merged

sortFacts :: [Term] -> [Term]
sortFacts = nub

deriveFacts :: Database -> [(String, Int)] -> Map.Map (String, Int) [Term]
            -> (String, Int) -> [Term]
deriveFacts db tabled known key =
  let subDb = db
        { dbClauses = [ d | d <- dbClauses db, not (isTabledHead tabled d) ]
                    ++ concatMap (\(k, fs) -> map (\f -> Clause f []) fs)
                                (Map.toList known)
        }
      forClause c =
        [ fi
        | let headInsts = [ resolve (stSubst sol) (clauseHead c)
                          | sol <- runSolver (solve subDb (clauseBody c) initState) ]
        , fi <- headInsts
        , ground fi
        ]
  in nub (concatMap forClause
                    (filter (\c -> headKey (clauseHead c) == key) (dbClauses db)))

-- A resolved head that still mentions a variable is not a usable fact.
headKey :: Term -> (String, Int)
headKey (TStruct f as) = (f, length as)
headKey (TAtom n) = (n, 0)
headKey _ = ("", -1)

isTabledHead :: [(String, Int)] -> Clause -> Bool
isTabledHead tabled c = headKey (clauseHead c) `elem` tabled

ground :: Term -> Bool
ground term = case term of
  TVar _ -> False
  TStruct _ args -> all ground args
  _ -> True
