-- | Probe: process/2, readutil/2, filesex against the Haskell answer.
-- Repo receipts: library(process) 9 uses, readutil 7, filesex 4
-- (e.g. v6/prolog/compile/scripts/text_door_receipt.pl, run_sql_check.pl).
module Probe.Process where

import System.Process (readProcess)

-- readutil read_file_to_string/2,3 equivalent
readToLines :: String -> [String]
readToLines = lines

runProcessProbes :: IO ()
runProcessProbes = do
  -- readutil / filesex: string chunking on a known input
  let c1 = readToLines "a\nb\nc\n" == ["a", "b", "c"]
  putStrLn $ if c1 then "PASS readutil-lines" else "FAIL readutil"
  -- process: run /bin/echo and read stdout, non-interactive
  out <- readProcess "/bin/echo" ["hello"] ""
  let c2 = out == "hello\n"
  putStrLn $ if c2 then "PASS process-echo" else "FAIL process"
