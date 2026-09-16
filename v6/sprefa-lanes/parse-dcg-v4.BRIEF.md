# feature/parse-dcg-v4: finish the DCG migration (sol, continuation of v3)

## Where v3 stopped (your own prior work; base sha contains it)
parse_dl_dcg.pl (556 lines) parses rel declarations, modifiers, scalar
expressions, and simple rules. Verified standing:
`PARSE_PARITY mode=classic-vs-dcg total=677 parity=421 skips=256 diffs=0`.
Every remaining skip currently reports the generic
`nonterminal=unmigrated_statement/0` — the skip marker does not name the
construct that is actually missing.

## The work
1. Migrate the remaining grammar sections of
   v6/prolog/compile/parse_dl.pl into parse_dl_dcg.pl until
   `just parse-parity` reports skips=0 diffs=0. Preserve clause order,
   cuts, and every thrown error term EXACTLY, same as v3.
2. FIRST slice: replace the blanket unmigrated_statement/0 skip with
   per-construct markers (one named nonterminal per unmigrated section),
   so every later commit's parity line shows WHICH constructs remain.
   Commit that alone with the skip breakdown in the message.
3. Then per-slice commits (up to 8, prefix `prolog:`), each message
   carrying the verbatim PARSE_PARITY line. diffs MUST stay 0 in every
   commit; a diff is a parser bug, never an acceptable intermediate.
4. NEVER delegate any DCG path to parse_dl:* predicates. A prior lane
   died exactly that way; `rg -c 'parse_dl:' parse_dl_dcg.pl` staying 0
   is part of every commit's self-check.

## Files you own
- v6/prolog/compile/parse_dl_dcg.pl
- v6/prolog/compile/scripts/parse_parity.pl ONLY for the skip-marker
  naming mechanism (do not restructure the comparator)
Do NOT touch parse_dl.pl, use_resolve.pl (the seam is done), print_dl.pl,
0_generic_expand.pl.

## Setup (REQUIRED; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate
```bash
cd <worktree>/v6 && just parse-parity && just conformance && just plunit && just text-door && just roundtrip
```
Finish line: skips=0 diffs=0 with the toggle-off battery green. If a
construct genuinely cannot be expressed as a DCG rule, that is a
finding: FAILURE-REPORT-DCG-V4.md with the parse_dl.pl clause cited and
why, exit nonzero. Partial progress with accurate named-skip counts is
an acceptable final state; silence is not.

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-DCG-V4.md, exact command + output, exit NONZERO. Work is
  independently re-verified.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.

## Style
Comments state only constraints the code cannot show. Banned words, prose
and identifiers: provenance, substrate, load-bearing, regime, refusal.
Descriptive variable names, never single-letter.
