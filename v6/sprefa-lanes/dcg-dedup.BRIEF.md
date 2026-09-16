# refactor/dcg-dedup: shrink parse_dl_dcg.pl below classic, same outputs

## The number to move (measured 2026-08-11)
Comment-stripped non-whitespace chars: classic parse_dl.pl = 46,563;
parse_dl_dcg.pl = 46,698. The DCG rewrite saved ZERO code mass. Your job:
find the redundancy that number implies and close the gap from above —
target meaningfully BELOW 46,563, with identical outputs.

Measurement command (this exact spelling, quote it in every commit):
```bash
grep -v "^\s*%" v6/prolog/compile/parse_dl_dcg.pl | tr -d ' \t\n' | wc -c
```

## Where to hunt (hypotheses, verify before believing)
- Clauses that look near-identical modulo one token: factor into one
  parameterized nonterminal. Prolog makes this cheap: a nonterminal can
  take the varying atom as an argument.
- Migration scaffolding v3/v4 left behind: skip machinery, dead
  alternates, duplicated ws/token helpers, any nonterminal with exactly
  one caller that only forwards.
- Repeated literal-then-ws patterns: one combinator (`kw(Atom)`,
  `punct(Char)`) instead of inline lit_dcg+ws0 pairs everywhere.
- Term-construction {} blocks rebuilding the same shapes: shared
  builders.
Simplify the IMPLEMENTATION only. The language's grammar, its error
terms, and every parsed program term are FROZEN: this is a refactor, not
a redesign. Language/type-system changes need Chris and are out of scope.

## Files you own
- v6/prolog/compile/parse_dl_dcg.pl ONLY.
Do NOT touch parse_dl.pl, parse_parity.pl, use_resolve.pl, print_dl.pl.

## Gate (after EVERY commit, quote both lines)
```bash
cd <worktree>/v6 && just parse-parity   # must stay total=677 parity=677 skips=0 diffs=0
cd <worktree>/v6 && just conformance && just text-door && just roundtrip
```
The char measurement plus the parity line go in every commit message.
Up to 5 commits, prefix `prolog:`. Final commit message carries
before/after char counts.

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-DCG-DEDUP.md, exact command + output, exit NONZERO.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.
- No push, no PR; coordinator harvests.

## Style
Comment budget: max 2 consecutive lines, constraints only. Banned words,
prose and identifiers: provenance, substrate, load-bearing, regime,
refusal. Descriptive names, never single-letter.
