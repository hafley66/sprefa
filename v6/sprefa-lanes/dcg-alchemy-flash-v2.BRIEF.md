# refactor/dcg-flash-<X>: shrink the DCG parser below 29534, outputs frozen

## Mission
You are pass 1 of 2 in a 3-lane flash bake-off. Rewrite
v6/prolog/compile/parse_dl_dcg.pl SMALLER than 29534 non-ws chars while
every gate stays green. The file was already alchemized once
(46698 -> 29534, PR #169): DCG --> notation, call//N, tok//1 fusion,
univ-generic merges, table dispatch are all DONE. Your win must come from
moves the last pass missed. If you cannot find one that survives the
gate, a FAILURE-REPORT naming the moves you tried and why each broke
parity is a valid deliverable; exit nonzero.

## Read FIRST, in order
1. v6/prolog/compile/parse_dl_dcg.pl (1103 lines, the thing you shrink)
2. git log --oneline -6 -- v6/prolog/compile/parse_dl_dcg.pl (what was
   already done; commit messages carry char counts)
3. v6/dl/fixtures/golden-flex.dl6 (what the language is)

## Known-fatal moves (each broke frozen outputs last pass; do not retry)
1. Bare DCG terminals for punctuation: drops mark_furthest, drifts error
   columns on throwing fixtures (positions are part of parity).
2. Merging sh_decl_stmt's two clauses: classic records
   column_type_wrapper findings twice via reparse; a merged clause
   records once and diverges.
3. Leading-ws sepv for count(...) atom lists: accepts input classic
   throws on.
4. Cuts in enum_variants: clause-1-to-2 backtracking on trailing ';' is
   part of the accepted language.

## FORBIDDEN
Delegating to parse_dl:* (rg -c 'parse_dl:' stays 0); changing any
parsed program term, error term, or parse_dl_dcg_entry/5; language or
type-system changes (Chris-only); touching any file except
v6/prolog/compile/parse_dl_dcg.pl.

## Scoreboard (quote all three in every commit message)
```bash
grep -v "^\s*%" v6/prolog/compile/parse_dl_dcg.pl | tr -d ' \t\n' | wc -c   # beat 29534
cd <worktree>/v6 && just parse-parity   # MUST print total=677 parity=677 skips=0 diffs=0
cd <worktree>/v6 && time just conformance   # >2x wall regression = report it
```

## Setup (REQUIRED before gates; absolute cd each command; pnpm, never npm)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Final gate (all green before your last commit)
```bash
cd <worktree>/v6 && just parse-parity && just conformance && just text-door && just roundtrip
```

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT.
- Blocked or beaten: FAILURE-REPORT-FLASH.md with exact command + output,
  exit NONZERO. If reality deviates from this brief, STOP and report; do
  not improvise.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.
- Up to 4 commits, prefix `prolog:`. No push, no PR; coordinator judges.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Each trick carries a max-2-line comment naming it so a
reader can grep the technique. Descriptive names outside the tricks.
