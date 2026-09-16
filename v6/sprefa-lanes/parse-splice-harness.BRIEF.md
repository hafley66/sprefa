# feature/parse-splice-harness: same-inputs parity splice for the two parsers

## User decree 2026-08-11
The DCG parser experiment (sibling lane feature/parse-dcg-v2, new file
parse_dl_dcg.pl, toggle DL_PARSER=dcg) must be spliceable against the
classic parser WITH THE SAME INPUTS through the existing fast tests. You
build the comparator; the sibling builds the parser. DO NOT edit
parse_dl_dcg.pl or the toggle seam, that lane owns them.

## Integrity rail, stated because a prior lane violated it
Exiting rc=0 with a dirty tree, no commits, or red gates is a DEFECT.
Blocked means FAILURE-REPORT-SPLICE.md with the exact command + output and
a NONZERO exit.

## The work
1. v6/prolog/compile/scripts/parse_parity.pl (+ a .sh entry): for every
   .dl6 in the corpus (conformance fixtures, TEXT_DOOR fixtures,
   compile/dl_view, v6/dl/fixtures incl golden-flex and gen/pokeapi_gen
   if present), parse with BOTH parsers and compare:
   - both succeed -> program terms must be variants; print PARITY or the
     first structural diff (path into the term, both subterms).
   - both throw -> thrown terms must match; print the pair when not.
   - one succeeds, one throws -> always a reported diff.
   Exit 0 only on full parity over parseable-by-classic inputs; sections
   the DCG parser has not migrated yet (existence_error on a nonterminal)
   count and print as SKIP with the nonterminal name, never as parity.
2. A plunit group wrapping the same comparator over a small pinned subset
   so `just plunit` carries a fast splice check permanently.
3. Wire a `just parse-parity` recipe into v6/justfile calling the script.
4. If the sibling's parser or toggle has not landed when you finish the
   harness, the harness must still run: classic-vs-classic parity over the
   corpus (trivially green) proves the plumbing; say which mode your
   receipts came from.

## Files you own
- v6/prolog/compile/scripts/parse_parity.pl / .sh (new)
- the plunit group file you add under compile/test/
- one recipe block in v6/justfile
Do NOT touch parse_dl.pl, parse_dl_dcg.pl, print_dl.pl, compile.pl,
0_generic_expand.pl, golden-flex.dl6.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6 && just conformance && just plunit && just parse-parity
cd <worktree>/v6 && just typecheck && just tsv2-test
```

## Rails
- NEVER git merge / pull / rebase in the worktree.
- NEVER --no-verify. Up to 2 commits, prefix `prolog:`. Comment budget:
  max 2 consecutive comment lines per touched hunk.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
