{-# LANGUAGE TemplateHaskell, QuasiQuotes #-}
-- | Probe: term_expansion/2 against the Haskell answer: Template Haskell.
-- Repo case: books/v6/rel_island.pl lines 26-35: at load time every
-- `Head <- Body` clause is rewritten into a datalog fact plus a tabled twin.
-- The Haskell answer is a quasiquoter (TH) compiled ahead of the splice.
-- term_expansion fires inline during a consult; TH pays a staging tax: the
-- generator must live in a module compiled before any splice, and the `<-`
-- surface sugar must be written as something GHC can lex.
module Probe.Sugar where

import Probe.SugarDsl

-- the quasiquoter ran at compile time: `expanded` is a DClause value
expanded :: DClause
expanded = [dl|reach|]

runSugarProbes :: IO ()
runSugarProbes = do
  -- the TH splice produced a value at compile time, not runtime
  print expanded
  putStrLn "PASS term-expansion-via-TH (staged)"
