-- SEED HEADER. Signatures are the contract; bodies are yours. Change a
-- signature only by writing the reason in REPORT.md.

module Prolog.Term
  ( Var(..)
  , Term(..)
  , Subst
  , walk
  , resolve
  , occurs
  , unify
  , rename
  , variantOf
  , freeVars
  , tAtom
  , nilList
  , isList
  , asList
  , mkList
  , standardCompare
  , occursIn
  ) where

import Data.List (sortBy, nubBy)
import Data.Map.Strict (Map, (!))
import qualified Data.Map.Strict as Map

-- Variables carry a rename generation so a clause can be used twice in one
-- proof without its variables colliding.
data Var = Var { varName :: String, varGen :: Int }
  deriving (Eq, Ord, Show)

data Term
  = TVar Var
  | TAtom String
  | TInt Integer
  | TStruct String [Term]
  deriving (Eq, Ord, Show)

type Subst = Map Var Term

-- walk s t: follow variable bindings at the ROOT of t until t is a non-variable
-- or an unbound variable. Does not descend into arguments.
walk :: Subst -> Term -> Term
walk subst (TVar v) = case Map.lookup v subst of
  Nothing -> TVar v
  Just term -> walk subst term
walk _ term = term

-- resolve s t: walk, then recurse into every argument. The full instantiation.
resolve :: Subst -> Term -> Term
resolve subst (TVar v) = case Map.lookup v subst of
  Nothing -> TVar v
  Just term -> resolve subst term
resolve subst (TStruct functor args) =
  TStruct functor (map (resolve subst) args)
resolve _ term = term

occursIn :: Var -> Term -> Bool
occursIn v (TVar u) = v == u
occursIn v (TStruct _ args) = any (occursIn v) args
occursIn _ _ = False

-- occurs s v t: does v appear anywhere in the resolved form of t.
occurs :: Subst -> Var -> Term -> Bool
occurs subst v term = occursIn v (resolve subst term)

-- unify s a b: standard first-order unification.
--   walk both sides
--   var/var    -> bind one to the other
--   var/term   -> occurs check first, then bind
--   atom/atom  -> equal names
--   struct     -> same functor, same arity, fold unify over the argument pairs
--   otherwise  -> Nothing
-- The occurs check is ON here. SWI's is OFF by default and flag-switched in
-- books/v6/algos/unify_hm.pl; that difference is a REPORT.md line.
unify :: Subst -> Term -> Term -> Maybe Subst
unify subst left right =
  case (walk subst left, walk subst right) of
    (TVar x, TVar y)
      | x == y -> Just subst
      | otherwise -> bind subst x (TVar y)
    (TVar x, other) -> bind subst x other
    (other, TVar y) -> bind subst y other
    (TAtom n1, TAtom n2) -> if n1 == n2 then Just subst else Nothing
    (TInt n1, TInt n2) -> if n1 == n2 then Just subst else Nothing
    (TStruct f1 a1, TStruct f2 a2)
      | f1 == f2 && length a1 == length a2 -> unifyArgs subst (zip a1 a2)
      | otherwise -> Nothing
    _ -> Nothing
  where
    bind subst' var term
      | occurs subst' var term = Nothing
      | otherwise = Just (Map.insert var term subst')
    unifyArgs subst' [] = Just subst'
    unifyArgs subst' ((p, q) : rest) =
      unify subst' p q >>= \subst'' -> unifyArgs subst'' rest

-- rename gen t: stamp every variable in t with gen, for fresh clause instances.
rename :: Int -> Term -> Term
rename gen (TVar v) = TVar (Var (varName v) gen)
rename gen (TStruct functor args) = TStruct functor (map (rename gen) args)
rename _ term = term

freeVars :: Term -> [Var]
freeVars term = nubBy (==) (go term)
  where
    go (TVar v) = [v]
    go (TStruct _ args) = concatMap go args
    go _ = []

-- Standard order of terms, approximating SWI's ordering over the ground
-- atoms/numbers that the fixtures actually sort. Variables, then numbers,
-- then atoms, then compound; within type by tag/name/arity.
standardCompare :: Term -> Term -> Ordering
standardCompare (TVar _) (TVar _) = EQ
standardCompare (TVar _) _ = LT
standardCompare _ (TVar _) = GT
standardCompare (TInt n1) (TInt n2) = compare n1 n2
standardCompare (TInt _) _ = LT
standardCompare _ (TInt _) = GT
standardCompare (TAtom name1) (TAtom name2) = compare name1 name2
standardCompare (TAtom _) _ = LT
standardCompare _ (TAtom _) = GT
standardCompare (TStruct fun1 args1) (TStruct fun2 args2) =
  compare (fun1, args1) (fun2, args2)

-- variantOf a b: the =@= of unify_hm.pl. Equal up to a consistent renaming of
-- variables, in BOTH directions. Alpha-equivalence by canonical names.
variantOf :: Term -> Term -> Bool
variantOf left right = canonicalize left == canonicalize right
  where
    canonicalize term =
      let vars = freeVars term
          table = Map.fromList
            (zip (map varName vars) [ vstr i | i <- [0 ..] ])
      in go table term
    go table (TVar v) = TAtom (table ! varName v)
    go table (TStruct functor args) = TStruct functor (map (go table) args)
    go _ term = term
    vstr i = "typevar" ++ show i

tAtom :: String -> Term
tAtom = TAtom

nilList :: Term
nilList = TStruct "[]" []

isList :: Term -> Bool
isList (TStruct "[]" []) = True
isList (TStruct ":" [_ , _]) = True
isList _ = False

asList :: Term -> [Term]
asList (TStruct "[]" []) = []
asList (TStruct ":" [head_, tail_]) = head_ : asList tail_
asList _ = error "asList: not a list"

mkList :: [Term] -> Term
mkList = foldr (\head_ tail_ -> TStruct ":" [head_, tail_]) nilList

sortTerms :: [Term] -> [Term]
sortTerms terms = nubBy eq (sortBy standardCompare terms)
  where
    eq a b = standardCompare a b == EQ
