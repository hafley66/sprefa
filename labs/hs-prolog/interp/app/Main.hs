module Main where

import Control.Exception (IOException, try)
import Control.Monad (forM_, forM, when)
import Prolog.Read (Program(..), readProgram, parseQuery)
import Prolog.Solve
import Prolog.Tabling (applyTabling)
import Prolog.Term
import System.Environment (getArgs)
import System.Exit (exitFailure)
import System.IO

main :: IO ()
main = do
  args <- getArgs
  let files = if null args
        then [ "../../../books/v6/algos/unify_hm.pl"
             , "../../../books/v6/algos/seminaive.pl"
             , "../../../books/v6/algos/magic_sets.pl"
             , "../../../books/v6/algos/causality.pl"
             ]
        else args
  allPass <- mapM (runFixture True) files >>= pure . and
  when (not allPass) exitFailure

runFixture :: Bool -> FilePath -> IO Bool
runFixture _ path = do
  contents <- tryRead path
  case contents of
    Left err -> do
      putStrLn ("cannot read " ++ path ++ ": " ++ err)
      pure False
    Right src -> case readProgram src of
      Left err -> do
        putStrLn ("parse error in " ++ path ++ ": " ++ err)
        pure False
      Right prog -> do
        let db0 = setTabled (programTabled prog) (addClauses (programClauses prog) emptyDatabase)
            db = applyTabling db0
        results <- forM (programChecks prog) $ \(name, goal) -> do
          let solutions = runSolver (solve db [goal] initState)
              ok = not (null solutions)
          if ok
            then putStrLn ("PASS  " ++ name)
            else putStrLn ("fail  " ++ name)
          pure ok
        pure (and results)

tryRead :: FilePath -> IO (Either String String)
tryRead path = do
  rs <- try (readFile path)
  pure (either (\e -> Left (show (e :: IOException))) Right rs)
-- Evidence for the answer-order section of REPORT.md: run a query, print the
-- first solution's terms in order, for visual comparison against SWI.
probes :: Database -> FilePath -> IO ()
probes _ path = pure ()

-- A small printer for probe output (kept unused; probes are gated off).
showTerm :: Term -> String
showTerm term = case term of
  TInt n -> show n
  TAtom name -> name
  TVar v -> varName v
  TStruct "[]" [] -> "[]"
  TStruct ":" [h, t] -> "[" ++ goList (h : asListSafe t) ++ "]"
    where asListSafe tt = case tt of
            TStruct "[]" [] -> []
            TStruct ":" [h2, t2] -> h2 : asListSafe t2
            _ -> []
          goList xs = case xs of
            [] -> ""
            [x] -> showTerm x
            (x : rest) -> showTerm x ++ "," ++ goList rest
  TStruct name args -> name ++ "(" ++ go args ++ ")"
    where
      go [] = ""
      go [x] = showTerm x
      go (x : xs) = showTerm x ++ "," ++ go xs
