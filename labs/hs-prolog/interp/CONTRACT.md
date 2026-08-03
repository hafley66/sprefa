# Lane L1: a tiny Prolog in Haskell on LogicT

## Question this lane answers

Can `logict` (Control.Monad.Logic) carry a real Prolog kernel: unification with
occurs check, clause indexing, backtracking, `findall`, negation as failure,
and cut? And when the kernel runs the repo's own graded Prolog algorithms, do
the answer sets match SWI's, in the same order?

## Base

First action, from the worktree root:

    git merge --ff-only a7108169

Expected output: `Already up to date.` Anything else: STOP and write REPORT.md
saying so. Do not work around it.

## Ownership

You own `labs/hs-prolog/interp/**` and `REPORT.md` at the worktree root. Touch
nothing else in the repo. Two other lanes are running against sibling paths;
edits outside your subtree are a defect, not a bonus.

## No-cheat rules

These are the point of the lab. Breaking one voids the result.

1. Do NOT depend on, vendor, read, or transcribe any existing Prolog / miniKanren
   / logic-interpreter implementation: the Hackage `prolog` package, `hs-prolog`,
   `logic-classes`, any `*kanren*` package, or a copied WAM. If you have such an
   implementation memorized, you still write yours from the operational
   semantics.
2. Allowed dependencies: `base`, `logict`, `containers`, `mtl`, `transformers`,
   `text`. Nothing else without writing the reason in REPORT.md.
3. `parsec`/`megaparsec` are allowed ONLY for the surface-syntax reader, and only
   after you write the build-vs-buy paragraph naming what a hand reader would
   cost. The solver itself is yours.
4. Every core function gets its type signature written BEFORE its body, with a
   pseudo-code comment under the signature saying what it does. Keep those
   comments in the landed file only where they state a constraint the code
   cannot show.

## Deliverable

A cabal project at `labs/hs-prolog/interp/` with:

- `src/Prolog/Term.hs`   terms, variables, substitution, unification, occurs check
- `src/Prolog/Solve.hs`  the LogicT solver, database, builtins
- `src/Prolog/Read.hs`   enough surface syntax to load the fixture clauses
- `app/Main.hs`          the grader: prints `PASS  <name>` / `fail  <name>` per check
- `REPORT.md` at the worktree root

## Grading fixtures: the repo's own algorithms

These files are in your worktree and are graded under SWI today by
`swipl -q -l <file> -g go -g halt`, printing one `PASS` line per check. Run each
under SWI first and paste the output into REPORT.md as the reference answer.
Then make your kernel produce the same answers.

| fixture | what it exercises | required |
|---|---|---|
| `books/v6/algos/unify_hm.pl` | unification as the engine, `occurs_check` flag, `=@=` | yes |
| `books/v6/algos/seminaive.pl` | `findall/3`, `sort/2`, `ord_subtract/3`, `ord_union/3`, cut in `loop/3` | yes |
| `books/v6/algos/magic_sets.pl` | `:- table`, recursion through a cycle, negation as failure | yes |
| `books/v6/algos/causality.pl` | tabled closure | if time |
| `books/v6/algos/marble.pl` | DCG both directions | if time; report the verdict either way |

Transcribing a fixture's clauses into a Haskell-embedded term DSL is acceptable
ONLY if `src/Prolog/Read.hs` cannot parse it yet; when you do that, say so in
REPORT.md per fixture and state what the reader is missing.

`:- table` has no free ride: state whether you implemented tabling, faked it by
memoizing one predicate, or left `magic_sets.pl` non-terminating. A non-terminating
honest answer beats a passing fake one.

## Grading command

    cd labs/hs-prolog/interp && cabal build && cabal run -v0 interp-grade

The FIRST `cabal build` may take minutes (it compiles dependencies) and is
exempt from the repo's 10-second law. Every graded RUN after that must finish
under 10 seconds; if one does not, that is a finding, and you report the number.

`ghc` and `cabal` are already installed at `/opt/homebrew/bin`. The Hackage index
is fresh. `swipl` is at `/opt/homebrew/bin/swipl`.

## Comparison section, required in REPORT.md

For each of these, a claim plus the evidence that produced it:

1. Where LogicT's `MonadLogic` interface (`msplit`, `interleave`, `>>-`, `once`,
   `ifte`) maps onto Prolog control, and where it does not. Cut is the hard case:
   say exactly which of Prolog's cut semantics you got and which you did not.
2. Fair vs depth-first search: SWI is depth-first and loops on left recursion.
   Show a query where `interleave` finds an answer that SWI's order does not, and
   one where fair search costs you something.
3. Line counts: your kernel vs the fixture's SWI line count for the same work.
4. What SWI gives free that you had to build (arithmetic, `sort/2`, assoc,
   exceptions, the occurs-check flag being global and mutable).
5. Answer-ORDER equality, not just set equality. Report both.

## Style laws (repo-wide, enforced)

- No em dashes anywhere, prose or code.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime. Use source, base layer, critical, mode.
- Comments state only constraints the code cannot show. No change-log narrative,
  no dates, no restating the next line.
- Descriptive names, never single-letter, in every snippet shown to the reader.
  Inside idiomatic Haskell code short names are fine where the file already does
  that; do not invent a house style mid-file.

## REPORT.md format

    # L1 hs-prolog interp: REPORT
    ## Base proof
    <the git merge --ff-only output, verbatim>
    ## What runs
    <the grader output, verbatim, plus the SWI reference output>
    ## Fixture table
    <fixture | SWI answer | my answer | same set? | same order? | notes>
    ## The comparison (the 5 items above)
    ## What I could not do
    <every gap, named. This section being empty is itself a claim.>
    ## No-cheat statement
    <name every source you consulted, and confirm rules 1-3 held>
