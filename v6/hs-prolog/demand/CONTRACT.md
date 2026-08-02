# Lane L3: what SWI powers does this repo actually use, and does Haskell have them

## Question this lane answers

Two sibling lanes are building small things in Haskell. This lane answers the
scaling question: if the Prolog layer of this repo (14,090 lines under
`v6/prolog/`, plus 1,898 lines of teaching algorithms under `books/v6/`) had to
move to Haskell, WHICH SWI powers would have to be replaced, and does an
equivalent exist. Answer per power, with a compiled probe, not a claim.

## Base

First action, from the worktree root:

    git merge --ff-only a7108169

Expected: `Already up to date.` Anything else: STOP, write REPORT.md, do not
work around it.

## Ownership

You own `labs/hs-prolog/demand/**` and `REPORT.md` at the worktree root. This
lane makes NO edits to repo code. Two sibling lanes are running against sibling
paths.

## Starting receipts (verified by the coordinator, extend them, do not trust them blindly)

Library imports across all `*.pl` in the repo, counted:

    54 lists      26 apply     9 process    7 readutil   7 pairs
     4 ordsets     3 plunit    3 filesex    3 aggregate  2 ugraphs
     2 prolog_xref 2 assoc     1 time       1 prolog_source
     1 format      1 check     1 between

`:- table` appears in 6 places across the repo. `library(ugraphs)` is used in
exactly one module, `v6/prolog/0_graph.pl`.

## Deliverable

`labs/hs-prolog/demand/DEMAND.md`, plus a cabal project of PROBES at
`labs/hs-prolog/demand/probes/`, plus `REPORT.md` at the worktree root.

### DEMAND.md, table 1: the feature inventory

One row per distinct SWI power the repo actually uses. Find them by reading, and
give a count and at least one `file:line` for each. Start from this list and
extend it; a power you cannot find in the repo gets dropped from the table, and
saying so is a result:

    term_expansion/goal_expansion   DCG (-->/2, phrase/2,3)   :- table
    findall/bagof/setof             forall/2                  aggregate_all/3
    assoc                           ordsets                   pairs
    cut                             \+ (negation as failure)  catch/throw
    =@= (variant)                   occurs_check flag         copy_term/2
    functor/arg/=../univ            atom/number type tests    format/2,3
    assert/retract                  module system             plunit
    operator declarations (op/3)    string/atom handling      between/3
    process/readutil/filesex        prolog_xref

### DEMAND.md, table 2: the Haskell answer per row

| power | repo uses | Haskell answer | probe | verdict |
|---|---|---|---|---|

`verdict` is exactly one of: `direct` (a library gives it with the same
semantics), `encodable` (you can express it, at a named cost in lines or types),
`hostile` (the idiom does not survive the port; say what breaks), `absent`.

Every row whose verdict is `direct` or `encodable` MUST cite a probe: a compiled,
running Haskell snippet in `probes/` that demonstrates it. A row with no probe is
`unproven`, and unproven rows go in their own section. This is the rule that makes
the lane worth running: a plausible-sounding table with no compiler behind it is
the failure mode.

### DEMAND.md, section 3: the three that decide it

Write a paragraph each, with the probe.

1. **term_expansion.** The repo compiles a language with it
   (`v6/prolog/1_expansion.pl`, 268 lines, and `books/v6/rel_island.pl` embeds a
   datalog island inside a Prolog file). Haskell's equivalent is Template Haskell
   or a quasiquoter. Show one, and say what the compile-time story costs.
2. **Tabling.** SWI's `:- table` gives terminating fixpoints on left-recursive
   rules for free. Show what Haskell needs for the same thing on the same rule
   set. `books/v6/algos/magic_sets.pl` and `books/v6/algos/causality.pl` are the
   two live cases in the repo.
3. **Unbound-mode predicates.** `v6/prolog/0_graph.pl` declares modes in comments
   (`+Edges, -Graph`); real Prolog predicates run in several modes, and
   `books/v6/algos/marble.pl` is one grammar that PARSES and PRINTS. Show the
   Haskell answer for one bidirectional grammar, or state that it costs two
   functions and name the library that avoids that.

### DEMAND.md, section 4: the honest verdict

Three to six sentences. Not a recommendation, a finding: what a port would cost
and what it would buy, in the terms above. Do not say "it depends".

## Build-vs-buy is a repo law

Any place your answer is "write our own", you first name the Hackage candidates
you checked and why each one does not fit. No one-line dismissals.

## Toolchain

`ghc` 9.14.1 and `cabal` 3.16.1.0 are at `/opt/homebrew/bin`, Hackage index is
fresh, `logict`/`containers`/`fgl`/`algebraic-graphs`/`mtl` verified to build
together on this machine. `swipl` is at `/opt/homebrew/bin/swipl`; use it to
check any claim about what SWI actually does. The first `cabal build` is exempt
from the repo's 10-second law; graded runs are not.

## Style laws (repo-wide, enforced)

- No em dashes anywhere.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Tables and file:line over prose. Under-word everything.
- No claim without a command or a file:line behind it.

## REPORT.md format

    # L3 hs demand: REPORT
    ## Base proof
    <git merge --ff-only output, verbatim>
    ## Probe build output
    <verbatim>
    ## Table 2 verdict counts
    <direct / encodable / hostile / absent / unproven, with numbers>
    ## The three that decide it
    <one line each, pointing into DEMAND.md>
    ## What I could not do
    <every gap, named>
