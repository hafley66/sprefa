# LANE: dd aggregate.group carries a bare column, bindings carry alias.column

You are a lane agent. You own ONE arc. Read this whole file before typing.

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT. Do not work around a blocked
command with archive/tar/copying/--no-verify. A permission denial ends the
approach.

## WORKTREE SETUP BEFORE YOUR FIRST COMMIT
The pre-commit rail strands every fresh worktree. Five lanes hit this. Do all
three BEFORE committing anything, and NEVER pass `--no-verify`:
1. copy the prebuilt binary `v6/sprefa-extract/target/release/extract` from the
   main tree into this worktree at the same path
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## THE DEFECT, ONE SHAPE, 31 OF 33 CORPUS ERRORS
Measured last session: 33 runtime errors across the dd corpus. 31 are the SAME
shape. `aggregate.group` in the emitted plan JSON carries a BARE column name,
while `bindings` in the same plan produce `alias.column`. The consumer resolves
against the binding namespace and the bare name does not resolve.

This is the cheapest lever on the board: one shape, 31 plans move.

## THE CITED SITES
```
v6/prolog/compile/6_emit_dd_plan.pl:173   reduce_arrangement_dict(Arrangements, Arrangement, GroupCols, ValueCols)
v6/prolog/compile/6_emit_dd_plan.pl:178   aggregate:_{kind:AggregateKinds, group:GroupCols, value:ValueCols}
v6/prolog/compile/6_emit_dd_plan.pl:272   reduce_arrangement_dict(Arrangements, ArrId, GroupCols, ValueCols) :-
v6/prolog/compile/6_emit_dd_plan.pl:273       member(arr(ArrId, _Ref, GroupCols, ValueCols, signed), Arrangements).
```
`GroupCols` and `ValueCols` come straight out of the `arr/5` term unqualified
and reach the JSON unqualified.

The alias namespace they must agree with is built here:
```
v6/prolog/compile/6_emit_dd_plan.pl:221   bindings_json([], Dict) :- Dict = _{}.
v6/prolog/compile/6_emit_dd_plan.pl:222   bindings_json([binding(Alias, Ref) | Rest], Dict)
v6/prolog/compile/6_emit_dd_plan.pl:502   positive_bindings(PositiveUses, Bindings)
v6/prolog/compile/6_emit_dd_plan.pl:505   positive_bindings([use(Ref, _, pos, _) | Rest], Index, [binding(Alias, Ref) | More])
```

## YOUR JOB
Make `aggregate.group` (and `aggregate.value` if it has the same shape, CHECK,
do not assume) carry the same `alias.column` spelling the bindings produce.
Resolve the alias from the SAME binding list the plan already emits, so there
is ONE alias derivation, never two that can drift apart.

READ THIS BEFORE YOU PICK AN APPROACH: `ARCH.pl:745` records an earlier
emitter-groupby defect that was FIXED TWICE because the first fix's fixture
only exercised ONE of two call sites. The lesson is recorded there verbatim.
Find every call site that emits a group/value column list. Prove you found
them all, in your report, by listing them.

## FILES YOU OWN (nobody else touches these this wave)
```
v6/prolog/compile/6_emit_dd_plan.pl
v6/prolog/compile/test/6_emit_dd_plan.test.pl
v6/prolog/compile/test/dd/
```
A CONCURRENT LANE OWNS `v6/prolog/0_type_plane.pl`, `v6/prolog/lower.pl`,
`v6/prolog/compile/registry.pl`, `v6/prolog/conformance/body.pl`,
`v6/prolog/compile/scripts/0_json_arrival.pl`. DO NOT EDIT ANY OF THOSE.
If your change needs one, STOP AND REPORT instead of editing.

## EXPECTED MOVEMENT (verify, never claim)
- grade.sh fixture half goes 2/3 -> 3/3
- bytes move on ~31 plans
Report the ACTUAL numbers you measured. If you get a different count than 31,
report the number you measured and say so plainly. Do not bend a number to
match this brief.

## FAIL-FIRST RECEIPT, REQUIRED
Before the fix, add a fixture that is RED for the RIGHT reason: a reduce
operator whose `aggregate.group` fails to resolve against `bindings`. Paste
the exact failure text. Then show it GREEN after. A report with no red-then-
green transcript is rejected.

## SABOTAGE RECEIPT, REQUIRED
After green, revert the alias qualification on purpose, show the fixture goes
RED, restore. Paste both transcripts.

## ANTI-CHEAT TABLE
| banned | why |
|--------|-----|
| `--no-verify` on any commit | the rail is the gate; a permission denial ends the approach |
| widening a fixture's expected value to match what the code emits | that is deleting the test |
| special-casing the fixture's own rel/column names | fix the shape, not the sample |
| a second independent alias derivation | two derivations drift; ARCH.pl:745 is that exact story |
| skipping/`@ignore`-ing a red test | KNOWN RED list is the ONLY allowed red |
| claiming a number you did not run | every number in your report is pasted tool output |
| editing files outside YOUR OWN list | disjoint ownership, a concurrent lane holds the rest |

## A LANE CAN EXIT rc=0 WITH A RED GATE AND ZERO COMMITS
That happened last session. Check your own gate output before reporting done.
rc=0 is not evidence.

## GATE (run all, paste output)
```
cd v6/prolog && swipl -g go -t halt ARCH.pl
bash grade.sh                                 # locate it; the dd grade leg
cd v6/tsv2 && bash scripts/sweep.sh
just green-all
```
Battery baseline to match or beat: conformance 281/0, plunit 276,
TEXT_DOOR 196/196/0, tsv2 128/1skip, store 74/74, dl 96/96.
The dd-grade ratchet exits 1 on newly byte-clean fixtures BY DESIGN, refusing
to silently absorb them. If that happens, record the fixtures in the ratchet
as a separate commit and say which ones.

## KNOWN RED (pre-existing, NOT yours, do not fix, do not count as failure)
See `.github/CI-KNOWN-RED.md` for every red leg with its exact failure text.
Read it BEFORE reporting anything as broken.

## STYLE LAWS (enforced, inline so you need no judgment)
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  OR identifiers.
- The word "refusal" is banned in prose; say TODO or "not built yet".
- Comment budget: comments state ONLY constraints the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next
  line. History belongs in git.
- dl variable names are descriptive, never single-letter, in every snippet.
- Construct names use ONLY rxjs, prolog, or SQL vocabulary. "support" is banned.
- Never a per-row write; collect the set, one insert.
- Colocated consistency: inside a file, follow that file's existing style.

## COMMIT OFTEN
A prior lane lost an entire run to a machine sleep. Commit each green step.

## REPORT
Write `REPORT.md` at the worktree root. Required sections:
1. the FULL list of call sites that emit a group/value column list, proving
   you found them all (ARCH.pl:745's lesson)
2. red-then-green transcripts (fail-first)
3. sabotage transcript
4. every gate command with its pasted output
5. the measured plan-movement count and the grade.sh fixture score
6. anything you did NOT do and why
Then stop. Do not open a PR. Do not spawn subagents; lanes never fan out.
