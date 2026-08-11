# refactor/dcg-stack: reapply losers' surviving moves onto the 26486 winner

## Mission
Main now holds the round-2 winner (PR #172, 26486 non-ws chars). Three
losing branches hold verified-parity techniques the winner may not have.
Reapply each move that is (a) absent from the winner's file and (b) still
a win on it. Every move lands as its OWN commit with parity green, so a
bad interaction bisects to one commit. Outputs frozen throughout.

## Move inventory (read the diffs, not just these summaries)
- branch refactor/dcg-flash-a (1 commit): spec-record fusion + unified
  column readers.
- branch refactor/dcg-flash-b (4 commits): col-type + wrappers + sh_decl
  merge shape; cst_body reuse of computed input names; build_path shared
  term builder; fill_free inline + identity-guard merges.
- branch refactor/dcg-flash-c (2 commits): decl/host coltype + fill_slots
  + escape map + sh_head merges; bind/cmp shared bin_item infix shape.
Read each: git log -p 9a258d86..refactor/dcg-flash-<x> --
v6/prolog/compile/parse_dl_dcg.pl (from the repo root of your worktree).
The winner already contains: b_setval var global, sep//2+args//2,
map_tree/4, #/@/~ sigil operators, kw//1, infix_item//2, pair-table
escape/record, sh_head//2. Many loser moves target seams the winner
already rewrote or deleted — expect a large fraction to be moot; SAY so
per move rather than forcing them.

## Known-fatal (unchanged from prior rounds; do not retry)
1. Bare DCG terminals for punctuation (mark_furthest / error columns are
   part of parity).
2. Merging sh_decl_stmt's clauses INTO ONE (the double
   column_type_wrapper record must survive; winner's sh_head//2 shape is
   the legal version).
3. Leading-ws list combinators for count(...) atom lists.
4. Cuts in enum_variants.
5. decl_b_column_type and host_col_type differ ONLY in a cut: merging
   them passes parity while silently changing the accepted language.
   FORBIDDEN (language change).

## Scoreboard (quote all three per commit)
grep -v "^\s*%" v6/prolog/compile/parse_dl_dcg.pl | tr -d ' \t\n' | wc -c   # start 26486, must go DOWN per commit
cd <worktree>/v6 && just text-door && bash prolog/compile/scripts/parse_parity.sh   # total=677 parity=677 skips=0 diffs=0 (text-door FIRST or the corpus is 411)
cd <worktree>/v6 && time just conformance

## Final gate
cd <worktree>/v6 && just parse-parity && just conformance && just text-door && just roundtrip

## Deliverable
- One commit per reapplied move, prefix `prolog:`, chars quoted.
- STACK-REPORT.md at worktree root: table move x source-branch x verdict
  (REAPPLIED -N chars / MOOT winner-already-has / REJECTED reason).
- No moves reapplied cleanly = report with all-MOOT table, exit 0 with
  the report committed.

## Rails
Setup: cd <worktree>/v6/tsv2 && pnpm install; cd <worktree>/v6/sprefa-store/js && pnpm install;
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract.
Files you own: v6/prolog/compile/parse_dl_dcg.pl + STACK-REPORT.md only.
NEVER git merge/pull/rebase (reading other branches via git log -p is
fine; checking them out is not). NEVER --no-verify. No push, no PR.
rc=0 with red gates or dirty tree is a DEFECT. If reality deviates from
this brief, STOP and report. Banned words, prose and identifiers:
provenance, substrate, load-bearing, regime, refusal.
