# fix/converter-nullable-arrays: emit option(list(...)), G2 gap 4 -> 0

## Continuation: a machine crash killed the prior attempt mid-work
Your worktree already carries UNCOMMITTED partial work from that attempt:
15 deleted lines in `v6/tsv2/scripts/openapi_to_dl6.ts` (the gap
bookkeeping) with NO replacement wrap yet, plus regenerated artifacts.
Read `git diff` FIRST and decide: keep the partial deletions if they are
correct, or `git checkout -- <file>` and redo cleanly. Either is fine;
say which you chose in the commit message. Nothing was committed, so
there is no bad history to unwind.

## Mission
The converter drops nullability on array properties citing "dl6 has no
nullable-array type". That claim is STALE: the coordinator probed the
compiler 2026-08-11 and
`rel tags_holder(id: int, tags: option(list(int))).` compiles clean
(OPTLIST_OK, full COMPILE-TRACE). Emit `option(list(X))` /
`option(json_list(X))` for nullable arrays, delete the G2 drop path, and
prove the spelling with a conformance fixture.

## You are pass 1 of 2. Favor plain code.

## Do, in order
1. `v6/tsv2/scripts/openapi_to_dl6.ts`: the wrap-suppression around
   :310-315 returns bare `base` when the property is nullable and base is
   a list. Wrap instead: `option(base)`. Remove the `nullableArrayGaps`
   bookkeeping (~:144, 202-203, 232) and its report rows; update the G2
   prose in the report emitter to state the spelling is emitted (one
   line).
2. New conformance fixture
   `v6/prolog/conformance/fixtures/13_option_list_columns.pl` (follow the
   shape of neighboring numbered fixtures): a rel with an
   `option(list(int))` column and an `option(list(text))` column, seeded
   facts including an absent (null) list and a present list, one derived
   rel reading them. Descriptive test names
   (`option_list_column_roundtrips_null_and_present`).
3. Regenerate: `cd <worktree>/v6/tsv2 && bash scripts/sweep.sh`. Your
   fixture must land in the manifest with `bucket=compiled`; quote that
   row in the commit message.

## Setup (required, absolute cd, pnpm never npm)
cd <worktree>/v6/tsv2 && pnpm install
cd <worktree>/v6/sprefa-store/js && pnpm install
cd <worktree>/v6/sprefa-extract && cargo build --release --features cli --bin extract

## Gates (all green before final commit; quote outputs verbatim)
- cd <worktree>/v6/tsv2 && npx tsx scripts/openapi_roundtrip_check.ts
  -> ROUNDTRIP PASS with nullable:786/0/0 AND the report's G2 dropped
  count 0 (was 4)
- grep -cE "option\((json_)?list\(" <worktree>/v6/tsv2/gen/pokeapi_gen.dl6
  -> at least 4
- cd <worktree>/v6 && just conformance && just text-door

## Rails
- rc=0 with dirty tree, no commits, or red gates is a DEFECT. Blocked ->
  FAILURE-REPORT-NULLARR.md, exact command + output, exit NONZERO.
- If the compiler rejects `option(list(...))` anywhere in YOUR fixture,
  STOP and report the exact throw — do not patch the compiler; you own
  the converter script and the fixture only.
- NEVER git merge/pull/rebase. NEVER --no-verify. Up to 3 commits, prefix
  `tsv2:`. No push, no PR; coordinator judges.
- If reality deviates from this brief, STOP and report; do not improvise.

## Style
Banned words, prose and identifiers: provenance, substrate, load-bearing,
regime, refusal. Comments state only constraints the code cannot show.
