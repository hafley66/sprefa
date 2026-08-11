# refactor/dcg-alchemy: leetcode the DCG parser with prolog black magic

## Mission (user decree 2026-08-11)
Rewrite v6/prolog/compile/parse_dl_dcg.pl to be as SMALL and as
prolog-idiomatic as skill allows, outputs frozen. This is a bake-off:
other agents hold the same brief on other branches; the best verified
result wins. Style points are real points: the user asked for "as much
prolog black magic as we can alchemy".

## Read FIRST, in this order
1. v6/dl/fixtures/golden-flex.dl6 — what the language IS (17 sections,
   every construct exercised; the coverage gate asserts each one).
2. v6/prolog/compile/parse_dl_dcg.pl — the implementation you replace,
   677/677 corpus parity with classic parse_dl.pl.
3. Any prolog research in the repo: `ls plans/ | grep -i prolog`,
   v6/prolog/rulings.pl, v6/prolog/ARCH.pl headers.

## Legal alchemy (verify each survives the gate; cite what you used)
Parameterized nonterminals; pushback/lookahead; phrase/3 fusion;
`string_code`/code-arithmetic char classes instead of member chains;
goal_expansion/term_expansion compile-time macros; operator declarations
for internal combinator DSLs; assoc/dict dispatch tables replacing clause
ladders; call//N higher-order nonterminals. FORBIDDEN: delegating any
path to parse_dl:* (rg -c 'parse_dl:' must stay 0); changing ANY parsed
program term, error term, or the entry signature
parse_dl_dcg_entry/5; language or type-system changes (Chris-only).
Readability may lose to density ONLY where a comment (max 2 lines)
states the trick's name; a reader must be able to grep the technique.

## The scoreboard (quote all three in every commit message)
```bash
grep -v "^\s*%" v6/prolog/compile/parse_dl_dcg.pl | tr -d ' \t\n' | wc -c   # chars: beat 46698, then beat the other lanes
cd v6 && just parse-parity    # MUST stay total=677 parity=677 skips=0 diffs=0
cd v6 && time just conformance    # parse speed regression >2x = report it
```

## Setup (REQUIRED before gates; absolute cd each command)
```bash
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract
```

## Gate (final, all green)
```bash
cd <worktree>/v6 && just parse-parity && just conformance && just text-door && just roundtrip
```

## Files you own
v6/prolog/compile/parse_dl_dcg.pl ONLY. Nothing else changes.

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-ALCHEMY.md, exact command + output, exit NONZERO.
- NEVER git merge / pull / rebase in the worktree. NEVER --no-verify.
- Up to 6 commits, prefix `prolog:`. No push, no PR; coordinator judges.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Descriptive names outside the tricks; where a trick
demands terseness, the 2-line comment names it.
