-- | Probe: DCG (-->/2) and phrase/2,3 against the Haskell answer.
-- Repo case: books/v6/algos/marble.pl, one grammar that PARSES and PRINTS.
-- The Haskell answer splits it into two functions: a parser (parsec) and a
-- pretty-printer (generator) over the same AST. That is the "two functions"
-- cost of section 3.3: unification-based bidirectional DCG becomes two
-- bespoke passes joined by a shared type.
module Probe.DCG where

import Text.ParserCombinators.Parsec
import Data.Char (toLower)

-- AST shared by both directions
data Ev = At Int Char | Done Int deriving (Show, Eq)

-- PRINTS events -> marble string  (the "generation" direction)
printMarble :: [Ev] -> String
printMarble = go 0
  where
    go :: Int -> [Ev] -> String
    go _ []                = ""
    go tick (At t c : rest) = dashes (t - tick) ++ [toLower c] ++ go (t + 1) rest
    go tick (Done t : _)    = dashes (t - tick) ++ "|"
    dashes n | n <= 0 = ""
             | otherwise = "-" ++ dashes (n - 1)

-- PARSES marble string -> events (the "recognition" direction)
-- go carries the absolute tick so At/Done get their true position
parseMarble :: String -> [Ev]
parseMarble s =
  case parse (p 0) "" s of
    Left e  -> error (show e)
    Right a -> a
  where
    p :: Int -> GenParser Char () [Ev]
    p tick =
      (char '|' >> return [Done tick])
      <|> (char '-' >> p (tick + 1))
      <|> do c <- anyChar
             if c >= 'a' && c <= 'z'
                then ((At tick c :) <$> p (tick + 1))
                else unexpected (show c)
      <|> return []

-- the fresh-check: print then parse round-trips
roundTrip :: [Ev] -> Bool
roundTrip evs = parseMarble (printMarble evs) == evs

runDCGProbes :: IO ()
runDCGProbes = do
  -- marble.pl check(parse): "ab--c|" -> events
  let parsed = parseMarble "ab--c|"
  let c1 = parsed == [At 0 'a', At 1 'b', At 4 'c', Done 5]
  -- marble.pl check(print): events -> "ab--c|"
  let printed = printMarble [At 0 'a', At 1 'b', At 4 'c', Done 5]
  let c2 = printed == "ab--c|"
  putStrLn $ if c1 then "PASS dcg-parse" else "FAIL dcg-parse"
  putStrLn $ if c2 then "PASS dcg-print" else "FAIL dcg-print"
