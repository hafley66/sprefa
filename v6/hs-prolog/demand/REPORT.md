# L3 hs demand: REPORT

## Base proof

    $ git merge --ff-only a7108169
    Already up to date.

## Probe build output

Clean build, `ghc 9.14.1`, `cabal 3.16.1.0`, deps logict/containers/fgl/mtl/
transformers/text/template-haskell/parsec, `-Wall`, zero warnings:

    Resolving dependencies...
    Build profile: -w ghc-9.14.1 -O1
    Configuring library for prolog-demand-probes-0.1.0.0...
    Building library for prolog-demand-probes-0.1.0.0...
    [ 1 of 11] Compiling Probe.Collections ...
    [ 2 of 11] Compiling Probe.DCG        ...
    [ 3 of 11] Compiling Probe.Exceptions ...
    [ 4 of 11] Compiling Probe.Graph      ...
    [ 5 of 11] Compiling Probe.Plunit     ...
    [ 6 of 11] Compiling Probe.Printf     ...
    [ 7 of 11] Compiling Probe.Process    ...
    [ 8 of 11] Compiling Probe.Quant      ...
    [ 9 of 11] Compiling Probe.SugarDsl   ...
    [10 of 11] Compiling Probe.Sugar      ...
    [11 of 11] Compiling Probe.Tabling    ...
    Configuring executable 'probes' ...
    [1 of 1] Compiling Main             ...
    [2 of 2] Linking .../probes/probes

Run: `cabal run -v0 probes` -> 24 PASS, 0 FAIL:

    PASS findall       PASS setof        PASS bagof       PASS forall
    PASS between       PASS tabling-fixpoint(cycle-closed)
    PASS tabling-cold  PASS tabling-selfreach
    PASS dcg-parse     PASS dcg-print
    PASS pairs-keysort PASS assoc        PASS ordsets
    PASS catch-recover PASS catch-clean
    PASS term-expansion-via-TH (staged)
    PASS graph-toposort PASS graph-closure PASS graph-cycle-detect
    PASS format-printf
    "double_works PASS"  "lists_sorted PASS"
    PASS readutil-lines PASS process-echo

## Table 2 verdict counts

direct 15, encodable 10, hostile 2, absent 1, plus 5 probe-less rows
(`=@=`, copy_term/2, cut, occurs_check, assert/retract) that count as
unproven under the probe rule.

## The three that decide it

1. term_expansion -> Template Haskell quasiquoter; the cost is the staging
   split (generator must live in its own compiled module) plus re-lexing the
   `<-` sugar; Probe.Sugar. See DEMAND.md section 3.1.
2. Tabling -> explicit Data.Set fixpoint loop on the same left-recursive
   reach rules; terminates on the cycle, cold component stays cold;
   Probe.Tabling. See DEMAND.md section 3.2.
3. Unbound modes -> two functions (parsec parse + printer) on one AST type;
   marble both directions verified under swipl and in Probe.DCG.
   See DEMAND.md section 3.3.

## What I could not do

- Five powers have a stated verdict but NO compiled probe behind them
  (counted unproven for the probe rule): alpha-variant `=@=`, copy_term/2
  renaming, cut semantics, occurs-check unification, dynamic assert/retract
  clause store. All need a hand-written equivalent I did not reach. They are
  in DEMAND.md table 2 as encodable/hostile but without a probe.
- Did not build probe-tools-internal or prove `prolog_xref` has any Haskell
  equivalent; those rows are absent (no probe), and a static-analyzer port
  would be its own lane.
- The bidirectional DCG round-trip (print then re-parse) is only argued
  structurally, not asserted as a probe check; the two single-direction
  checks (parse, print) are the compiled evidence.
- Coordinator receipt corrections: `v6/prolog/1_expansion.pl` is 57 lines,
  not 268 (268 is `labs/json_syntax/3_lists.pl`). Full `v6/prolog/` tree is
  37,074 lines; the 14,090 figure is only the 22 top-level modules.
  `books/v6/` is 1,898, correct. The `use_module(library(...))` import table
  in the contract matched a `use_module`-scoped grep exactly; the larger
  `library(...)` prose grep does not (ARCH.pl and comments contain many),
  which is why the contract's `:- table` is 6 real decls, not 13 raw text
  hits.
