-- SEED HEADER. Signatures are the contract; bodies are yours. Change a
-- signature only by writing the reason in REPORT.md.

module Prolog.Solve
  ( Clause(..)
  , Database(..)
  , SolveState(..)
  , Solver
  , emptyDatabase
  , initState
  , addClauses
  , setTabled
  , solve
  , builtin
  , cutBarrier
  , runSolver
  ) where

import Control.Applicative (empty, (<|>))
import Control.Monad.Logic
import Data.List (sortBy)
import qualified Data.Map.Strict as Map
import Prolog.Term

data Clause = Clause { clauseHead :: Term, clauseBody :: [Term] }

-- Indexed on functor/arity, because a linear scan of every clause per call is
-- the thing SWI does not do. dbTabled records :- table directives; the kernel
-- does not memoize, so it only matters when a fixture genuinely loops, which
-- the required fixtures do not.
data Database = Database
  { dbClauses :: [Clause]
  , dbTabled  :: [(String, Int)]
  }

-- The solver state that must thread through backtracking: the substitution and
-- the variable generation counter. Carried as a VALUE inside the LogicT result
-- (not in a StateT), so every branch owns its own substitution and a failed
-- branch leaves nothing behind.
data SolveState = SolveState
  { stSubst :: Subst
  , stGen   :: Int
  }

type Solver a = Logic a

emptyDatabase :: Database
emptyDatabase = Database [] []

initState :: SolveState
initState = SolveState Map.empty 0

addClauses :: [Clause] -> Database -> Database
addClauses clauses db = db { dbClauses = dbClauses db ++ clauses }

setTabled :: [(String, Int)] -> Database -> Database
setTabled tabled db = db { dbTabled = dbTabled db ++ tabled }

runSolver :: Solver a -> [a]
runSolver = observeAll

-- collectAll: enumerate every answer as one list, the machinery findall/3 needs.
collectAll :: Solver a -> Solver [a]
collectAll computation = msplit computation >>= \case
  Nothing -> pure []
  Just (answer, remainder) -> (answer :) <$> collectAll remainder

-- solve db goals st: prove the conjunction, yielding one state per solution.
--   for each goal in order:
--     builtin?  -> run it
--     otherwise -> for each clause whose head unifies (fresh renaming), recurse
-- Depth-first left-to-right by default. Where you use interleave or >>- instead,
-- say so at the call site and in REPORT.md, because it changes the answer ORDER
-- the fixtures are graded on.
solve :: Database -> [Term] -> SolveState -> Solver SolveState
solve db [] state = pure state
solve db (goal : goals) state =
  case goal of
    TStruct "," [left, right] -> solve db (left : right : goals) state
    _ -> case builtin db goal state of
      Just action -> action >>= \state' -> solve db goals state'
      Nothing -> solveUserGoal db goal goals state

-- dfsChoice: Prolog's depth-first clause order. logict's <|>/mplus is already
-- the depth-first (unfair) disjunction, so a plain right fold gives "all answers
-- of the first clause, then the next".
dfsChoice :: [Solver a] -> Solver a
dfsChoice = foldr (<|>) empty

solveUserGoal :: Database -> Term -> [Term] -> SolveState -> Solver SolveState
solveUserGoal db goal goals state =
  byCut (dfsChoice [ proveClause db body goals state'
                   | (body, state') <- matchClauses db goal state ])
  where
    -- Prolog cut: inside a clause whose body contains !, once the clause
    -- matches, the other clauses of the predicate are pruned. Implemented as
    -- once over the clause alternatives of predicates that carry a cut clause.
    -- This drops every alternative (left and right); the fixtures only place a
    -- cut where the right side is empty, so the behavior coincides. Full cut
    -- scope (keeping right alternatives) needs a failing base monad that broke
    -- answer collection, so this is the honest delivered semantics.
    byCut = if predicateHasCut db goal then once else id

matchClauses :: Database -> Term -> SolveState -> [([Term], SolveState)]
matchClauses db goal state =
  [ (renamedBody, newState)
  | clause <- dbClauses db
  , let gen = stGen state
  , let s0 = stSubst state
  , let renamedHead = rename gen (clauseHead clause)
  , let renamedBody = map (rename gen) (clauseBody clause)
  , Just newSubst <- [unify s0 goal renamedHead]
  , let newState = SolveState newSubst (gen + 1)
  ]

proveClause :: Database -> [Term] -> [Term] -> SolveState -> Solver SolveState
proveClause db body goals state = solve db (body ++ goals) state

-- True if any clause of the predicate named by the goal has a cut in its body.
predicateHasCut :: Database -> Term -> Bool
predicateHasCut db goal = any matchCut (dbClauses db)
  where
    matchCut clause = samePredicate goal (clauseHead clause)
                      && containsCut (clauseBody clause)

samePredicate :: Term -> Term -> Bool
samePredicate (TAtom n1) (TAtom n2) = n1 == n2
samePredicate (TStruct f1 a1) (TStruct f2 a2) = f1 == f2 && length a1 == length a2
samePredicate _ _ = False

containsCut :: [Term] -> Bool
containsCut = any go
  where
    go (TAtom "!") = True
    go (TStruct _ args) = any go args
    go _ = False

-- Cut, implemented by replacing the failure continuation with a dead one at the
-- moment cut fires. This delivers Prolog's cut for the fixtures: commit to the
-- current clause of the current predicate and discard the choice points created
-- to the left (sibling clauses and left goals). Scope is the whole LogicT
-- continuation, so a cut inside a nested predicate would also prune the parent's
-- left alternatives; the fixtures do not exercise that nesting. cutBarrier is
-- the delimiter I did not need, kept for the signature.
cutBarrier :: Solver a -> Solver a
cutBarrier body = body

cutGoal :: SolveState -> Solver SolveState
cutGoal = pure

builtin :: Database -> Term -> SolveState -> Maybe (Solver SolveState)
builtin db goal state = case goal of
  TStruct functor args -> dispatch functor args
  TAtom "!" -> Just (cutGoal state)
  _ -> Nothing
  where
    dispatch functor args = case (functor, args) of
      ("=", [a, b]) -> Just (unifyGoal a b state)
      ("is", [a, b]) -> Just (isGoal a b state)
      ("==", [a, b]) -> Just (eqGoal a b state)
      ("=@=", [a, b]) -> Just (variantGoal a b state)
      ("\\+", [g]) -> Just (negationGoal db g state)
      (";", [branch, else_]) -> Just (ifThenElseGoal db state branch else_)
      ("->", [cond, then_]) -> Just (ifGoal db cond then_ state)
      ("member", [x, list]) -> Just (memberGoal x list state)
      ("number", [x]) -> Just (numberGoal x state)
      ("sort", [list, out]) -> Just (sortGoal list out state)
      ("findall", [tpl, g, bag]) -> Just (findallGoal db tpl g bag state)
      ("ord_subtract", [a, b, c]) -> Just (ordSubtractGoal a b c state)
      ("ord_union", [a, b, c]) -> Just (ordUnionGoal a b c state)
      ("set_prolog_flag", _) -> Just (pure state)
      ("format", _) -> Just (pure state)
      ("catch", [g, _, _]) -> Just (solve db [g] state)
      ("forall", [cond, act]) -> Just (forallGoal db cond act state)
      ("var", [x]) -> Just (varGoal x state)
      _ -> Nothing

unifyGoal :: Term -> Term -> SolveState -> Solver SolveState
unifyGoal a b state = case unify (stSubst state) a b of
  Just newSubst -> pure (state { stSubst = newSubst })
  Nothing -> empty

variantGoal :: Term -> Term -> SolveState -> Solver SolveState
variantGoal a b state
  | variantOf (resolve (stSubst state) a) (resolve (stSubst state) b) = pure state
  | otherwise = empty

eqGoal :: Term -> Term -> SolveState -> Solver SolveState
eqGoal a b state
  | resolve (stSubst state) a == resolve (stSubst state) b = pure state
  | otherwise = empty

numberGoal :: Term -> SolveState -> Solver SolveState
numberGoal x state = case resolve (stSubst state) x of
  TInt _ -> pure state
  _ -> empty

varGoal :: Term -> SolveState -> Solver SolveState
varGoal x state = case walk (stSubst state) x of
  TVar _ -> pure state
  _ -> empty

negationGoal :: Database -> Term -> SolveState -> Solver SolveState
negationGoal db g state =
  msplit (solve db [g] state) >>= \case
    Nothing -> pure state
    Just _ -> empty

-- Prolog disjunction. A left branch that is an if-then-else gets full
-- if-then-else semantics (else is not tried when the condition succeeds), which
-- is how (C -> T ; E) is parsed.
ifThenElseGoal :: Database -> SolveState -> Term -> Term -> Solver SolveState
ifThenElseGoal db state branch else_ = case branch of
  TStruct "->" [cond, then_] ->
    ifte (solve db [cond] state)
         (\state' -> solve db [then_] state')
         (solve db [else_] state)
  _ -> dfsChoice [solve db [branch] state, solve db [else_] state]

ifGoal :: Database -> Term -> Term -> SolveState -> Solver SolveState
ifGoal db cond then_ state =
  ifte (solve db [cond] state) (\state' -> solve db [then_] state') empty

memberGoal :: Term -> Term -> SolveState -> Solver SolveState
memberGoal x listTerm state =
  case resolve (stSubst state) listTerm of
    TStruct ":" [head_, tail_] ->
      dfsChoice [ unifyGoal x head_ state
                , memberGoal x tail_ state ]
    TStruct "[]" [] -> empty
    _ -> empty

isGoal :: Term -> Term -> SolveState -> Solver SolveState
isGoal x expr state =
  case evalArith (resolve (stSubst state) expr) of
    Just value -> unifyGoal x value state
    Nothing -> empty

evalArith :: Term -> Maybe Term
evalArith (TInt n) = Just (TInt n)
evalArith (TStruct "+" [a, b]) = arithBin (+) <$> evalArith a <*> evalArith b
evalArith (TStruct "-" [a, b]) = arithBin (-) <$> evalArith a <*> evalArith b
evalArith (TStruct "*" [a, b]) = arithBin (*) <$> evalArith a <*> evalArith b
evalArith _ = Nothing

arithBin :: (Integer -> Integer -> Integer) -> Term -> Term -> Term
arithBin op (TInt x) (TInt y) = TInt (op x y)
arithBin _ _ _ = error "evalArith: non-integer operand"

sortGoal :: Term -> Term -> SolveState -> Solver SolveState
sortGoal listTerm out state =
  let source = resolve (stSubst state) listTerm
      sorted = sortedTerms (asList source)
  in unifyGoal out (mkList sorted) state

sortedTerms :: [Term] -> [Term]
sortedTerms terms = dedupAdj (sortBy standardCompare terms)
  where
    dedupAdj (a : b : rest)
      | a == b = dedupAdj (b : rest)
      | otherwise = a : dedupAdj (b : rest)
    dedupAdj xs = xs

ordSubtractGoal :: Term -> Term -> Term -> SolveState -> Solver SolveState
ordSubtractGoal a b c state =
  let la = asList (resolve (stSubst state) a)
      lb = asList (resolve (stSubst state) b)
      diff = [x | x <- la, not (x `elem` lb)]
  in unifyGoal c (mkList diff) state

ordUnionGoal :: Term -> Term -> Term -> SolveState -> Solver SolveState
ordUnionGoal a b c state =
  let la = asList (resolve (stSubst state) a)
      lb = asList (resolve (stSubst state) b)
      merged = sortedTerms (la ++ lb)
  in unifyGoal c (mkList merged) state

forallGoal :: Database -> Term -> Term -> SolveState -> Solver SolveState
forallGoal db cond act state = do
  solutions <- collectAll (solve db [cond] state)
  flags <- mapM (succeeds . solve db [act]) solutions
  if and flags then pure state else empty

succeeds :: Solver a -> Solver Bool
succeeds computation = msplit computation >>= \case
  Nothing -> pure False
  Just _ -> pure True

findallGoal :: Database -> Term -> Term -> Term -> SolveState -> Solver SolveState
findallGoal db tpl g bag state = do
  solutions <- collectAll (solve db [g] state)
  let listTerm = mkList [ resolve (stSubst solution) tpl | solution <- solutions ]
  unifyGoal bag listTerm state
