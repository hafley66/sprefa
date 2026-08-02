-- | Probe: format/2,3 (printf) against the Haskell answer: Text.Printf.
-- Repo uses format/2,3 per DEMAND.md receipts.
module Probe.Printf where

import Text.Printf (printf)

runPrintfProbes :: IO ()
runPrintfProbes = do
  -- SWI: format("PASS  ~w~n", [N])  (rel_island.pl:70)
  -- Haskell: printf "PASS  %s\n" ... same shape
  let s = printf "PASS  %s" "term_expansion"
  putStrLn $ if s == "PASS  term_expansion" then "PASS format-printf" else "FAIL format"
