# fix/decl-order-msort CONTINUATION: finish fix A, two red plunit tests

## Where you are
The previous attempt stalled after 5 hours stuck on the same two plunit
failures. Its diff is banked at sprefa-lanes/decl-order-attempt1.patch
(147 lines: 0_generic_expand.pl reordering, plunit edit, new fixture
conformance/fixtures/0_decl_order.pl). Read the ORIGINAL brief FIRST for the
full design and repro: sprefa-lanes/decl-order-fix-a.BRIEF.md. Apply the
patch if it helps (`git apply sprefa-lanes/decl-order-attempt1.patch` from
the worktree root), or start clean; either way you own the result.

## The two red tests, by name (everything else was green at 588/590)
1. `expansion_order:generic_e2e_declaration_permutation_is_byte_deterministic`
   (v6/prolog/compile/test/plunit_tests.pl). This failure is EXPECTED and the
   fix is to REWRITE THE TEST, per the original brief: under fix A,
   within-rel column order is program data. The rewritten test asserts
   (a) same input text -> byte-identical output, (b) permuting WHOLE decl
   statements -> byte-identical output, (c) permuting columns WITHIN a rel is
   a DIFFERENT program: assert both orders compile green, and assert their
   outputs differ only in column order, never assert byte equality.
2. `catalog_plane_rail:level_plane_family_corpus_counts`. A corpus-count
   rail: it counts constructs across the fixture corpus, and the new fixture
   0_decl_order.pl changed the counts. Read the test, recount, update the
   expected numbers to the measured reality (state old -> new in the commit
   message). If the count change is NOT explained by the new fixture, STOP
   and write the failure report: that would mean the reorder changed
   expansion output for an existing fixture, which fix A must not do.

## Everything else from the original brief still owed
- golden-flex GENERICS section (the arc's user-visible payoff)
- conformance fixture green for the FAILURE-REPORT.md repro program
- rulings.pl decision row; delete FAILURE-REPORT.md at repo root when its
  repro lives on as a fixture

## Non-negotiable rails
- NEVER git merge / pull / rebase in the worktree.
- Blocked -> FAILURE-REPORT-DECL-ORDER.md, exact command + output, exit
  NONZERO. Exiting 0 with a dirty tree or red gates is a defect.
- NEVER --no-verify. Extractor missing at commit time: build it with
  `cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract`

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just text-door && just roundtrip && just golden-flex
cd <worktree>/v6/tsv2 && bash scripts/sweep.sh
git checkout -- v6/prolog/compile/out/pokeapi_shape.ts
cd <worktree>/v6 && just typecheck && just tsv2-test
```
All green, no known-red exceptions: main is fully green as of 6a285e4b.

## Commit rail (commit-or-report)
Up to 3 commits, prefix `prolog:`. Comment budget: max 2 consecutive
comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
dl variable names descriptive, never single-letter.
