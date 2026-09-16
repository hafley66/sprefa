# LANE: a reference target with zero stored columns keeps its __id

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT. No archive/tar/copy workaround.
A permission denial ends the approach.

## WORKTREE SETUP BEFORE FIRST COMMIT (never `--no-verify`)
1. copy `v6/sprefa-extract/target/release/extract` from the main tree in
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## THE USER'S DECISION, VERBATIM
"null and empty array are not same, we must be able to express optional. why
can we not see that we have optional's here and have a parent row get minted,
shouldn't every table be getting a row id anyyways?"

The user is right, and the coordinator verified it by probe. Implement it.

## WHAT THE COORDINATOR ALREADY PROVED (reproduce these first, they are your
## fail-first receipt)

Probe A, `option(text)` on a reference target COMPILES GREEN today:
```
rel combo_pair(use_before: option(text), use_after: option(text)).
rel move_combo(id: int, normal: combo_pair).
```
emits, and note the parent KEEPS both columns:
```sql
CREATE TABLE "combo_pair" ("__id" INTEGER PRIMARY KEY, "use_before" INTEGER NOT NULL, "use_after" INTEGER NOT NULL, UNIQUE ("use_before", "use_after"))
```

Probe B, `option(<rel>)` with one ordinary column, COMPILES GREEN today:
```
rel combo_move(name: text, url: text).
rel combo_pair(label: text, use_before: option(combo_move)).
rel move_combo(id: int, normal: combo_pair).
```
emits, and note the column MOVES OUT to a companion keyed on the PARENT'S id:
```sql
CREATE TABLE "combo_pair" ("__id" INTEGER PRIMARY KEY, "label" INTEGER NOT NULL, UNIQUE ("label"))
CREATE TABLE "combo_pair__use_before" ("combo_pair_id" INTEGER NOT NULL, "combo_move_id" INTEGER NOT NULL, PRIMARY KEY ("combo_pair_id"))
```

Probe C, EVERY column is `option(<rel>)`, STOPS today. THIS IS THE BUG:
```
rel combo_move(name: text, url: text).
rel combo_pair(use_before: option(combo_move), use_after: option(combo_move)).
rel move_combo(id: int, normal: combo_pair).
```
exact text:
```
unsupported_construct: compiler refused rule 'reference_target_has_no_columns' for rel 'combo_pair/0' (reference_target_has_no_columns)
```
Run all three with:
```
bash v6/prolog/compile/scripts/compile_dl6.sh IN.dl6 OUT.ts
```

## THE DIAGNOSIS, CITED
Probe B proves parent association by `__id` ALREADY WORKS: the companion table
is keyed on `combo_pair_id`, the parent's `__id`, and `__id INTEGER PRIMARY
KEY` is emitted on every reference target unconditionally.

What breaks is narrower than the plan doc claims. `v6/prolog/lower.pl:2102`
emits `UNIQUE (~w)` unconditionally from `set_rel_pk_sql/6` (called at :2092).
With every column moved out, that column list is EMPTY, so the constraint
degenerates: `UNIQUE()` over zero columns means every row duplicates every
other row. That is the collapse. It is a degenerate constraint, NOT a missing
identity concept.

`plans/2026-08-11-option-list-rel-generic.md` frames this as "a zero-column row
has no full-row identity" and prices fork B-b as gaining "a third case" in the
identity rule. Re-read that section AFTER you have run the probes. The
coordinator's position, which you should verify or refute with evidence: the
`__id` is already the identity, so this is dropping a meaningless constraint,
not adding an identity case. IF YOU FIND EVIDENCE THE COORDINATOR IS WRONG,
SAY SO IN THE REPORT WITH THE CITATION. Do not implement something you have
proved incorrect.

## THE CHANGE
When a reference target's stored column list is empty:
```sql
CREATE TABLE "combo_pair" ("__id" INTEGER PRIMARY KEY)
```
No `UNIQUE` clause. Every arrival mints a fresh `__id`, which is what a rowid
table does by default and what the data wants: each move has its OWN
contest_combos, so deduping two parents onto one row is the defect.

Sites you will need:
| site | what it does now | what it needs |
|---|---|---|
| `v6/prolog/lower.pl:2092` | calls `set_rel_pk_sql/6` | must tolerate an empty column list |
| `v6/prolog/lower.pl:2102` | `CREATE TABLE ... ("__id" INTEGER PRIMARY KEY, <cols>, UNIQUE (<pk>))` | with zero columns, emit `("__id" INTEGER PRIMARY KEY)` only |
| `v6/prolog/lower.pl:2109` | `CREATE TEMP VIEW __ref_<name> AS SELECT t."__id", <cols>, <render> AS "__rendered"` | a zero-column rel renders `{}` |
| `v6/prolog/compile/0_generic_expand.pl:275-280` | throws `reference_target_has_no_columns(<rel>/0)` | remove or narrow, now that the case is built |

The arrival path must also stop looking for a content match that cannot exist.
FIND that site yourself and cite it in the report; do not guess at it.

## FILE OWNERSHIP, READ CAREFULLY, DEVIATION FROM THE USUAL RULE
A CONCURRENT LANE (`fix-json-null-is-none`) IS EDITING `v6/prolog/lower.pl`
RIGHT NOW, in the region around line 5015 and the four `IS NULL` sites.

You own ONLY these regions of `lower.pl`:
- the `rel_ddl/6` set-rel clause and its DDL emission, roughly lines 2060-2120
- the render-expression path reached from `:2109` (`relation_render_expr`)

DO NOT EDIT ANY OTHER PART OF `lower.pl`. Do not reformat it. Do not
reorder clauses. Do not touch anything past line 3000. The coordinator
resolves the merge and a stray edit elsewhere in the file costs a conflict.

You also own outright:
```
v6/prolog/compile/0_generic_expand.pl
v6/prolog/conformance/fixtures/          (new fixtures only, no edits to existing)
```

DO NOT TOUCH, other lanes hold them: `v6/prolog/0_type_plane.pl`,
`v6/prolog/compile/registry.pl`, `v6/prolog/conformance/body.pl`,
`v6/prolog/compile/scripts/0_json_arrival.pl`,
`v6/prolog/compile/6_emit_dd_plan.pl`, `v6/prolog/compile/test/`,
`v6/dl/grammar/`.
If your change needs one of those, STOP AND REPORT instead of editing.

## FAIL-FIRST RECEIPT, REQUIRED
Probe C above IS your fail-first. Paste its exact stop text BEFORE the fix,
then paste the compile going green AFTER, with the emitted DDL for
`combo_pair` showing `("__id" INTEGER PRIMARY KEY)` and no `UNIQUE`.

Then prove the thing that actually matters, which a DDL diff does NOT prove:
TWO different parents must reach TWO different rows. Write a fixture with two
`move_combo` rows, each with its own `combo_pair`, each carrying a different
`use_before`, and assert the two do not collapse onto one row. A fix that
emits the right DDL and still merges two parents is not a fix.

## SABOTAGE RECEIPT, REQUIRED
After green, restore the `UNIQUE` clause on purpose, show the two-parent
fixture goes RED, restore. Paste both transcripts.

## ANTI-CHEAT TABLE
| banned | why |
|---|---|
| `--no-verify` on any commit | the rail is the gate |
| deleting the `0_generic_expand.pl` stop without building the case | that turns a named stop into a silent wrong answer; the plan doc records that an earlier attempt at exactly this produced "GOAL FAILED, no ball" |
| widening a fixture's expected value to match output | that is deleting the test |
| a DDL-only receipt | prove two parents stay distinct, per above |
| editing `lower.pl` outside your two regions | a concurrent lane holds the rest of the file |
| claiming a number you did not run | every number is pasted tool output |

## A LANE CAN EXIT rc=0 WITH A RED GATE AND ZERO COMMITS
That happened last session. Check your own gate output before reporting done.

## GATE (run all, paste output)
```
cd v6/prolog && swipl -g go -t halt ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh
just green-all
```
Baseline to match or beat: conformance 281/0, plunit 276, TEXT_DOOR 196/196/0,
tsv2 128/1skip, store 74/74, dl 96/96.

## KNOWN RED (pre-existing, NOT yours)
`.github/CI-KNOWN-RED.md` lists every red leg with exact failure text. Read it
BEFORE reporting anything as broken.

## THE PAYOFF, VERIFY DO NOT ASSUME
This is the last blocker on pokeapi G1, currently 4 drops, all of them
`move_detail__contest_combos__normal` and `__super`
(`v6/tsv2/gen/pokeapi_gen.dl6:144-150`). Measure G1 after your fix and report
the number you actually got.

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose, say TODO or "not built
yet"; the literal existing identifier is fine to quote. Comments state ONLY
constraints the code cannot show, no change-log narrative, no dates, no arc
references. Descriptive variable names, never single-letter. Construct names
use ONLY rxjs, prolog, or SQL vocabulary. Surrogate keys law: stored rels key
on INTEGER ids, a composite TEXT PRIMARY KEY is a defect. Colocated
consistency inside a file.

## COMMIT OFTEN. A prior lane lost a whole run to a machine sleep.

## REPORT
`REPORT.md` at the worktree root: (1) the three probes before/after,
(2) the two-parent distinctness fixture red-then-green, (3) sabotage
transcript, (4) the arrival-path site you found, cited, (5) every gate command
with pasted output, (6) measured pokeapi G1 count, (7) whether you agree or
disagree with the coordinator's diagnosis and why, (8) what you did NOT do.
Do not open a PR. Do not spawn subagents; lanes never fan out.
