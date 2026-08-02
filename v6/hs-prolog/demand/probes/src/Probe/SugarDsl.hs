{-# LANGUAGE TemplateHaskellQuotes #-}
-- | The compile-time generator, split into its own module so it is compiled
-- before any splice. TH staging: a quasiquoter must be in a module that is
-- already compiled, which is exactly the cost term_expansion/2 does not pay
-- (it fires inline during the same consult).
module Probe.SugarDsl where

import Language.Haskell.TH
import Language.Haskell.TH.Quote

-- the value-level target: a datalog clause, like dl_rule/1 in rel_island
data DClause = DClause String deriving (Show, Eq)

dl :: QuasiQuoter
dl = QuasiQuoter { quoteExp = dlExp, quotePat = error "no pat", quoteType = error "no type", quoteDec = error "no dec" }

dlExp :: String -> Q Exp
dlExp src = return (ConE 'DClause `AppE` LitE (StringL src))
