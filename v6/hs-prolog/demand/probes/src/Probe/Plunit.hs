-- | Probe: plunit (:- begin_tests / end_tests) against the Haskell answer.
-- Repo: v6/prolog/compile/test/plunit_tests.pl and others.
module Probe.Plunit where

-- plunit = a test block with named assertions and an automated runner.
-- Haskell answer: a tiny runner over structured assertions, or a framework
-- (hspec/tasty). We show the shape: named checks plus a collect-and-report.
data T = T String Bool deriving (Show)

runTests :: [T] -> [String]
runTests = map (\(T n ok) -> if ok then unwords [n, "PASS"] else unwords [n, "FAIL"])

plunitProbes :: [T]
plunitProbes =
  [ T "double_works" ((2 :: Int) * 2 == 4)
  , T "lists_sorted" ([1, 2, 3] == sortL ([3, 1, 2] :: [Int]))
  ]
  where sortL = foldr insert []

insert :: Int -> [Int] -> [Int]
insert x [] = [x]
insert x (y : ys) | x <= y    = x : y : ys
                  | otherwise = y : insert x ys

runPlunitProbes :: IO ()
runPlunitProbes = do
  mapM_ print (runTests plunitProbes)
