module Main where

import Probe.Quant
import Probe.Tabling
import Probe.DCG
import Probe.Collections
import Probe.Exceptions
import Probe.Sugar
import Probe.Graph
import Probe.Printf
import Probe.Plunit
import Probe.Process

main :: IO ()
main = do
  putStrLn "== Quant (findall/bagof/setof/forall/between) =="
  runQuantProbes
  putStrLn "== Tabling (:- table fixpoint) =="
  runTablingProbes
  putStrLn "== DCG (-->/2, phrase) =="
  runDCGProbes
  putStrLn "== Collections (assoc/ordsets/pairs) =="
  runCollectionsProbes
  putStrLn "== Exceptions (catch/throw) =="
  runExceptionsProbes
  putStrLn "== Sugar (term_expansion / quasiquoter) =="
  runSugarProbes
  putStrLn "== Graph (ugraphs/fgl) =="
  runGraphProbes
  putStrLn "== Printf (format) =="
  runPrintfProbes
  putStrLn "== Plunit =="
  runPlunitProbes
  putStrLn "== Process/readutil =="
  runProcessProbes
