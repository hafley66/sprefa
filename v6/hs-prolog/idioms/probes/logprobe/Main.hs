module Main (main) where

import Colog.Core (LogAction, logStringStdout, (&>))

-- A structured log line: one field pair per signed step, rendered to stdout.
-- The field separation is explicit so the probe shows the shape of a real
-- co-log line.
main :: IO ()
main = do
  let logger :: LogAction IO String
      logger = logStringStdout
  "level=info component=logprobe msg=hello_from_logprobe" &> logger
