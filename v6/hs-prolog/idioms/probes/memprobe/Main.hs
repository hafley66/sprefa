module Main (main) where

import System.Environment (getArgs)

-- Build a real boxed list with explicit recursion so the optimizer cannot fuse
-- it away: it stays live and is walked. This gives the heap something to show.
downTo :: Int -> [Int]
downTo 0 = []
downTo n = n : downTo (n - 1)

mySum :: [Int] -> Int
mySum [] = 0
mySum (x : xs) = x + mySum xs

main :: IO ()
main = do
  args <- getArgs
  let n = case args of
        (x : _) -> read x
        [] -> 200000
  let xs = downTo n
  putStrLn ("len=" ++ show (length xs) ++ " total=" ++ show (mySum xs))
