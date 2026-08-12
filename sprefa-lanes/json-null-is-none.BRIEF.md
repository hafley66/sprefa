# LANE: json null IS none (user decision 2026-08-11)

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

## THE USER'S DECISION, VERBATIM
"i wnat json compat, so we must endure null, but not fucking sql null 3vl bc no
one agrees its a good idea"
then, ruling the spelling: "null can be synonym for none from optional, yea i
think"

=> JSON `null` IS the atom `none` from optional. It is NOT a new type. It is
NOT a SQL NULL. `option(T)` + `match` is the entire absence story and stays
unchanged. Three-valued logic never enters emitted SQL.

## WHAT IS ALREADY RIGHT (do not "fix" these)
- `v6/prolog/compile/scripts/0_json_arrival.pl:92` already folds `null` onto
  atom `none`. Correct under the decision. Leave it.
- `v6/prolog/conformance/body.pl:229` `json_patch_carries_null/1` already tests
  `Value == none` on the read side. Correct. Leave the semantics; you may need
  to update what it expects (see below).
- `option(T)` is an enum split, a some-table keyed on id (`lower.pl:882-886`).
  It stores ZERO SQL NULLs. Do not touch the option representation.

## THE TWO REAL DEFECTS, BOTH CITED
| # | site | current behavior | required |
|---|------|------------------|----------|
| 1 | `v6/prolog/0_type_plane.pl:707+` `canonical_json_text/2` | renders `none` as the STRING `"none"` | renders bare `null` |
| 2 | `v6/prolog/lower.pl:5015` | throws `json_patch_null_unruled`; emitter emits `json('json_patch_null_unruled')`, text that is not valid JSON so SQLite fails the statement | delete the refusal, the decision now exists |

Defect 1 is the one that breaks json round-trip. A document holding a null
currently comes back holding the four-character string "none".

## RENDERING RULE (decided, do not re-open)
A `none`-valued key EMITS `"key":null` and round-trips. Do NOT omit the key.
RFC 7386 merge-patch omission semantics are explicitly NOT what we want here;
json compat means `{"a":null}` -> read -> write -> `{"a":null}` byte-identical.

## THE 3VL BOUNDARY YOU MUST CLOSE
Measured this session, verbatim from sqlite3:
```
json_extract('{"a":null}','$.a') IS NULL   -> 1     <-- SQL NULL LEAKS IN HERE
json_type('{"a":null}','$.a')              -> null
json_type('{"b":1}','$.a')                 -> NULL  (absent key)
json_patch('{"a":1,"b":2}','{"a":null}')   -> {"b":2}
NULL = NULL                                -> NULL  (the banned 3VL)
NULL IS NULL                               -> 1     (2-valued, null-safe)
NULL IS 1                                  -> 0
EXPLAIN QUERY PLAN ... WHERE a IS 5        -> SEARCH t USING INDEX ta (a=?)
```
`json_extract` of a JSON null returns a SQL NULL. That is the one seam where
3VL can enter. Intercept it there so no SQL NULL ever reaches a comparison,
an aggregate, a UNIQUE column, `NOT`, or `IN`.

`IS` / `IS NOT` is null-safe 2-valued equality AND uses the index with the
identical query plan as `=` (proved above: SEARCH, not SCAN). If you need a
comparison to be null-safe, the two rows that decide it are:
```
v6/prolog/compile/registry.pl:248  expression('=='/2,   identity_comparison, 0, infix('='),  same_type).
v6/prolog/compile/registry.pl:249  expression('\\=='/2, identity_comparison, 0, infix('<>'), same_type).
```
Every comparison funnels through `comparison_operator_sql/5`
(`lower.pl:1781`). Change the registry rows, never a call site.
Do NOT touch `ordered_comparison` rows (:243-246, :253-254).

## FILES YOU OWN (nobody else touches these this wave)
```
v6/prolog/0_type_plane.pl
v6/prolog/lower.pl
v6/prolog/conformance/body.pl
v6/prolog/compile/registry.pl
v6/prolog/compile/scripts/0_json_arrival.pl
v6/prolog/conformance/fixtures/          (new fixtures here)
```
A CONCURRENT LANE OWNS `v6/prolog/compile/6_emit_dd_plan.pl` AND
`v6/prolog/compile/test/`. Do not edit those. If your change needs them, STOP
AND REPORT instead of editing.

## FAIL-FIRST RECEIPT, REQUIRED
Before the fix, write a fixture that is RED for the RIGHT reason and paste the
exact failure text into your report. Minimum two:
1. a document holding a json null round-trips byte-identically
2. a json null read back is `none` and a `match` on it takes the none arm
Then show them GREEN after. A report with no red-then-green transcript is
rejected.

## SABOTAGE RECEIPT, REQUIRED
After green, break the fix on purpose (e.g. revert `canonical_json_text/2` to
emit the string), show the fixture goes RED, restore. Paste both transcripts.

## ANTI-CHEAT TABLE
| banned | why |
|--------|-----|
| `--no-verify` on any commit | the rail is the gate; a permission denial ends the approach |
| widening a fixture's expected value to match what the code emits | that is deleting the test |
| `catch/3` around the new path to swallow a throw | hides the defect |
| skipping/`@ignore`-ing a red test | KNOWN RED list below is the ONLY allowed red |
| claiming a number you did not run | every number in your report is pasted tool output |
| editing files outside YOUR OWN list | disjoint ownership, a concurrent lane holds the rest |

## GATE (run all, paste output)
```
cd v6/prolog && swipl -g go -t halt ARCH.pl
cd v6/tsv2 && bash scripts/sweep.sh          # regenerates compile/out/manifest.json
just green-all
```
Battery baseline to match or beat: conformance 281/0, plunit 276,
TEXT_DOOR 196/196/0, tsv2 128/1skip, store 74/74, dl 96/96.
Byte-for-byte oracle parity on the tick log is the real gate.

## KNOWN RED (pre-existing, NOT yours, do not fix, do not count as failure)
See `.github/CI-KNOWN-RED.md` for every red leg with its exact failure text.
Read it BEFORE reporting anything as broken. `golden-flex` red on json_object/2
excused as 'registry status refused' is a KNOWN STALE excuse; if your work
happens to make it green, say so, but do not chase it.

## STYLE LAWS (enforced, inline so you need no judgment)
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime` in prose
  OR identifiers.
- The word "refusal" is banned in prose; say TODO or "not built yet". It stays
  only in literal code identifiers already present.
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
1. red-then-green transcripts (fail-first)
2. sabotage transcript
3. every gate command with its pasted output
4. the exact diff summary: files touched, lines, what each change does
5. anything you did NOT do and why
Then stop. Do not open a PR. Do not spawn subagents; lanes never fan out.
