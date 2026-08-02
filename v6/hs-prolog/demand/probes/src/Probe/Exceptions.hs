-- | Probe: catch/3, throw/1 against the Haskell answer.
-- SWI: catch(Goal, Catcher, Recover); throw(Term). Haskell: exceptions.
module Probe.Exceptions where

import Control.Exception

-- a custom exception type standing in for a Prolog error term
data PErr = PErr String deriving (Show)

instance Exception PErr

-- throw/1 : raise an error term
throwing :: String -> IO Int
throwing t = throwIO (PErr t)

-- catch/3 : run a goal; on the named error, run the recover goal
catching :: IO Int -> String -> (Int -> IO Int) -> IO Int
catching goal _name recover =
  (goal `catch` \(PErr _) -> recover 0)

runExceptionsProbes :: IO ()
runExceptionsProbes = do
  -- catch(atom_length(X, _), ..., recover): a failing goal is recovered
  r1 <- catching (throwing "type_error") "type_error" (\_ -> return (-1))
  let c1 = r1 == (-1)
  -- a clean goal passes through unchanged
  r2 <- catching (return 42) "type_error" (\_ -> return (-1))
  let c2 = r2 == 42
  putStrLn $ if c1 then "PASS catch-recover" else "FAIL catch"
  putStrLn $ if c2 then "PASS catch-clean" else "FAIL catch-clean"
