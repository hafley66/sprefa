# BRIEF: pokeapi gaps G1/G2, generic columns on nested ref targets

## Base
- Branch: `fix/pokeapi-generic-nesting`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `b580d627` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## One sentence
29 pokeapi columns fall to the `json` carrier because a rel used as a reference
TARGET cannot itself carry generic `option()`/`list()` columns; find out whether
that is a phase-order accident, a converter defect, or a real type-system
question, and bring back cited forks.

## HARD LAW, read it twice
**Language and type-system design happens with Chris in the room.** You do NOT
settle whether `option(list(<rel>))`, `option(json_list(_))`, or generic columns
on nested ref targets become legal spellings. You trace, measure, and present
forks with throw sites. The user rules. Implementing an already-measured
mechanical fix is fine; minting a new accepted spelling is not.

## The receipts you start from

Two named gaps, from `v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md:12-13`:

| gap | spelling | current behavior | throw site |
|---|---|---|---|
| G1 | ref-target rel carrying `option()`/`list()` columns | 29 columns dropped to `json` by the converter | `v6/prolog/0_type_plane.pl:128` `column_type_unknown` |
| G2 | `option(list(<rel>))` | not built | `v6/prolog/0_type_plane.pl:128` `column_type_unknown` |
| G2 | `option(json_list(_))` | not built | `v6/prolog/0_option_expand.pl:48` `option_element_type_unknown` |

`option(list(int))` and `option(list(text))` DO compile. Fixture:
`v6/prolog/conformance/fixtures/13_option_list_columns.pl:13-14`. Manifest row
`option_list_column_roundtrips_null_and_present`, bucket `compiled`. So the
option-over-list machinery exists; only the element kind stops it.

## The prior attempt, and why it is your strongest lead

A lane `fix/typedecl-mirror` ran and FAILED. Its report is at
`sprefa-lanes/typedecl-mirror.FAILURE-REPORT.md`. Read it in full. The important
claim:

> The compiler mirror fix passes the direct repro, conformance, TEXT_DOOR, and
> focused expansion tests. The converter's strict fallback still rewrites the
> affected columns before the compiler receives them.

That report blames `v6/tsv2/scripts/openapi_to_dl6.ts:276-278` and says
`applyStrictFalls` "unconditionally rewrites generic columns on ref targets to
json". **Check that claim against the code before you believe it.** The current
source at that location calls `probeRefTargets(candidates, byName)` and only
rewrites members of the returned `bad` set, which is a PROBE, not an
unconditional rewrite. One of these is wrong. Establish which.

Second unexplained number: main reports G1 = 29 dropped columns
(`POKEAPI_ROUNDTRIP_REPORT.md:69`); the mirror worktree reported
`75 + 4`. A fix that raised the drop count needs explaining before anything is
built on it.

The mirror branch was deleted. Do not hunt for it. Re-derive.

## Suspected root cause, stated as a hypothesis you must confirm or kill

Phase order, the same shape as the already-diagnosed enum defect at
`v6/prolog/ARCH.pl:930` (`enum_column_type_erased`). There, `type_decl/2` is
minted by the PARSER (`v6/prolog/compile/parse_dl.pl:834`
`normalize_relation_value_decls`) from `col_type` entries, while expansion runs
LATER (`v6/prolog/1_expansion.pl:69`), so
`v6/prolog/0_type_plane.pl:62 type_definitions/2` never sees the expanded name.

For generics the mirror already exists:
`v6/prolog/0_generic_expand.pl:254 retarget_type_decl_mirrors/2`, called at
lines 30 and 40. Determine what it retargets, what it misses, and whether a
ref-target rel with generic columns produces a `type_decl/2` mirror at all by
the time `type_definitions/2` runs. Cite line numbers.

## Work

1. **Minimal repro, in dl6, not through pokeapi.** Write the smallest program
   that reproduces G1: a rel with an `option()` or `list()` column, used as
   another rel's column type. Get the exact thrown term and the exact phase it
   is thrown from. Same for both G2 spellings. Three repros, three throw traces.
2. **Trace each one to its phase.** For each, state whether the stop encodes a
   real impossibility (storage, the print-values-never-ids law, three-valued
   logic) or unfinished work. Cite the code, not comments. Per the repo law, a
   comment is not the language.
3. **Re-derive the mirror fix** if step 2 says phase order. Land it ONLY if it
   is mechanical and every gate below stays green. If it changes which spellings
   compile, STOP and present the fork.
4. **Explain the 29 vs 75+4 discrepancy.** Run
   `cd v6/tsv2 && npx tsx scripts/openapi_roundtrip_check.ts` on your base first
   to get the number YOUR tree produces, before changing anything.
5. **Present the forks.** For each of the three spellings, one fork table:
   option / what it would store / what it costs / what law it touches / what
   would have to change and where. No recommendation ranking; the user ranks.

## Storage laws that constrain any answer you propose
Read `.claude/skills/sql-relational-design` and `.claude/skills/sqlite-costs`
BEFORE proposing any storage shape. Both are mandatory. The binding ones:
- Stored rels key on INTEGER ids. A composite TEXT PRIMARY KEY is a defect.
- Natural/composite TEXT keys live ONCE in a dictionary table.
- Measured: TEXT keys are 1.7-2.0x slower on identical tables.
- `0_type_plane.pl:115-119` records why the element-type CHECK is not emitted:
  CHECK constraints prohibit subqueries and `json_each` is a table function.
  Any list-element proposal has to answer that.
- `0_type_plane.pl:129-131` records why a relation ref inside a list is refused
  separately: ids in a list would enter the tick log, breaking the
  print-values-never-ids decision. Your G2 fork must address that directly.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/0_generic_expand.pl` | edit, only for a mechanical mirror fix |
| `v6/prolog/0_type_plane.pl` | edit, only if step 2 proves unfinished work |
| `v6/prolog/0_option_expand.pl` | edit, same condition |
| `v6/prolog/conformance/fixtures/**` | add red/green fixtures |
| `v6/tsv2/scripts/openapi_to_dl6.ts` | edit |
| `plans/2026-08-11-pokeapi-generic-nesting.md` | create, your deliverable |
| `v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md` | update numbers only if you moved them |

Forbidden: `v6/boop/src/**`, `v6/prolog/compile/parse_dl_dcg.pl` (another lane
owns it this session), `v6/labs/tree-sitter-door/**`, `chat_log/**`.

## Gates, every commit
```bash
cd v6 && just conformance      # 281/0
cd v6 && just plunit           # 276
cd v6 && just text-door        # 272/272/0
cd v6 && just green-all        # final, before you report done
cd v6/tsv2 && npx tsx scripts/openapi_roundtrip_check.ts   # report the number
```
The 10-second law: any single operation over 10s is a defect to investigate,
not a budget. Named exception: SCIP indexing.

## Deliverable
`plans/2026-08-11-pokeapi-generic-nesting.md` plus a matching
`plans/2026-08-11-pokeapi-generic-nesting.visual.human.unga.md` (plain words,
diagrams, zero citations, written for a reader with zero context). A plan
without the unga doc is undelivered.

The citation doc contains, in order:
1. The three repro programs, each with its verbatim thrown term and phase.
2. Confirm-or-kill on the phase-order hypothesis, with line numbers.
3. The 29 vs 75+4 explanation.
4. The three fork tables.
5. Gate output, verbatim.
6. Any code you landed, with the before/after gap count.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; an unbuilt construct is "TODO" or "not built
  yet". The word survives only in literal code identifiers and existing
  filenames.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references.
- Tables and lists over prose. Numbers come from tool output only.
- dl variable names are descriptive, never single-letter.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- N+1: never a per-row write; collect the set, one insert.
