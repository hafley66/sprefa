# BRIEF: option(list(<rel>)) via generic wrapper walking, no list-specific special case

## Base
- Branch: `feature/option-list-rel-generic`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `48fadfb3` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## USER DECISION 2026-08-11, verbatim
"get the option list rel done _please_ dont make bespoke as much as possible do
generic expansion of generics and such, dont invent a list specific autistic
cheat, go, now"

`option(list(<rel>))` becomes a legal spelling. The design is decided; you
implement it. The METHOD is also decided: **generic wrapper walking**. A patch
that adds `option` to `list_type_word/1`, or any other one-off widening of a
list-specific table, is REJECTED ON SIGHT even if every gate goes green.

## What is broken today

`rel span(start_offset: int, end_offset: int).`
`rel finding(path: text, spans: option(list(span))).`

stops with `unsupported_construct(column_type_unknown(span))`, thrown from
`0_program_check.pl:342-347`, reached via `check_supported_subset_expanded` at
`compile.pl:179`.

Cause, from `plans/2026-08-11-pokeapi-generic-nesting.md` (read it in full
first): `list_element_type_name/2` peels the four list flavors and never
`option` (`v6/prolog/compile/parse_dl_dcg.pl:637-646`, `list_type_word/1`;
classic door `v6/prolog/compile/parse_dl.pl:1052-1060`), so the element rel
never enters `ValueRelationNames` and gets no `type_decl/2` mirror. A later
check then reads a column type no rel declares.

Everything downstream ALREADY lowers. The prior lane verified this through the
classic door: companion split rel, minted list and member rels, `ref(span)` on
`member.value`, every key INTEGER. You are not building storage. You are making
one type walk see through wrappers it already knows about.

## The shape you build

There is one idea repeated in several places with different spellings: peel a
type expression's wrappers down to the name inside. Find every such site, name
the walk ONCE, and have every site call it.

Start by inventorying them. Known starting points, each to be confirmed by
reading, not by trusting this list:

| site | file | what it peels today |
|---|---|---|
| `list_element_type_name/2` | `compile/parse_dl_dcg.pl:637-646` | the four list flavors, never `option` |
| `list_element_type_name/2` | `compile/parse_dl.pl:1052-1060` | same, the classic door |
| `list_element_type/2` | `0_type_plane.pl:133+` | list element admissibility |
| `column_storage/3` | `0_type_plane.pl:115-131` | `json_list(Element)` arm |
| `scalar_element/1` | `0_option_expand.pl:51-55` | option element admissibility |
| `retarget_type_decl_mirror` | `0_generic_expand.pl:254-268` | the mirror rewrite |
| `columnRelRefs` | `v6/tsv2/scripts/openapi_to_dl6.ts` | already peels to any depth; read it, the TS side got this right first |

`columnRelRefs` is the existing proof that the wrapper-generic form works. It
peels to any depth and shipped in `01a350ed`. Match that shape on the Prolog
side.

Constraints on the walk:
- It terminates. A wrapper's argument is either a scalar name, a declared
  name, or another wrapper. Cite where the fixpoint bottoms out.
- It is ONE definition. If both parser doors need it and neither can import
  from the other, say so with the module lines and propose where it lives.
- Widening the walk must not widen what COMPILES beyond the decided spelling.
  A wrapper combination that was TODO before and is still TODO must still stop,
  with the same thrown term. Prove this with fixtures, both directions.

## SECOND USER DECISION, same message: "i want pokeapi done"

The target is **0 converter strict-mode drops**, not 12. That means
`option(<rel>)` on a reference target (8 columns) is IN SCOPE alongside
`option(list(<rel>))` (4 columns). Both spellings become legal.

`option(<rel>)` has a DIFFERENT cause, and the same anti-bespoke rule applies.
The option desugar `desugar_reference_option` (`0_option_expand.pl:84-92`,
`exclude/3` at `:87`) DELETES the column from `col_type` and moves it to a
companion split rel. `retarget_type_decl_mirror` (`0_generic_expand.pl:265`)
only rewrites a spec whose column is still findable, so a deleted column falls
through `:268` unchanged and the mirror keeps the raw `option(lang)`.
`mirror_column_type/3` already handles a RENAME; there is no clause for a
DELETION.

Do not bolt on a deletion special case beside the rename case. The mirror
rewrite has one job: keep the schema mirror agreeing with what the expansion
actually produced. Express rename and deletion as the same operation over the
expansion's own record of what it did, or say in the plan doc why they cannot
be one thing, with the code cited.

Receipt of done: `cd v6/tsv2 && npx tsx scripts/openapi_roundtrip_check.ts`
prints `dropped columns (G1): 0`. That command reads
`v6/dl/fixtures/POKEAPI_ROUNDTRIP_REPORT.md:69`'s number as its own output;
update the report's counts and its G1/G2 prose when you move them.

`option(json_list(_))` stays TODO. It needs a name-mangling decision first:
`option_enum_name/2`
(`0_option_expand.pl:81-82`) would mint the atom `'__opt_json_list(int)'` and
enum expansion concatenates variant rel names from it, so parentheses reach
DDL. Leave it stopping. Report what your walk would have done.

## Files you own
| path | permission |
|---|---|
| `v6/prolog/compile/parse_dl_dcg.pl` | edit |
| `v6/prolog/compile/parse_dl.pl` | edit |
| `v6/prolog/0_type_plane.pl` | edit |
| `v6/prolog/0_option_expand.pl` | edit |
| `v6/prolog/0_generic_expand.pl` | edit |
| `v6/prolog/conformance/fixtures/**` | add fixtures |
| `v6/prolog/compile/test/plunit_tests.pl` | add tests |
| `plans/2026-08-11-option-list-rel-generic.md` | create |

Forbidden: `v6/boop/src/**`, `v6/labs/**`, `chat_log/**`, `.github/**`.

## Fixtures. Both directions, every one.
| spelling | expected |
|---|---|
| `option(list(<rel>))` | COMPILES, round-trips null and present |
| `option(<rel>)` on a rel used as a reference target | COMPILES, round-trips null and present |
| `option(list(int))`, `option(list(text))` | still compile, unchanged |
| `list(<rel>)`, `list(int)`, `json_list(int)` on a ref target | unchanged from base |
| `option(json_list(_))` | still stops, same thrown term |
| a wrapper nest that was TODO on base | still stops, same thrown term |

Every fixture states the term it expects. A fixture that only asserts "compiles"
is not a fixture.

## Gates, every commit
```bash
cd v6 && just conformance     # 372 PASS, 0 FAIL on base
cd v6 && just parse-parity    # parity == total, skips=0, diffs=0
cd v6 && just plunit          # see the KNOWN RED below
cd v6 && just text-door       # compiled=272 byte_identical=272 failures=0
cd v6 && just green-all       # final
```

**KNOWN RED ON BASE, do not chase, do not fix:**
- `plunit`: `catalog_plane_rail:level_plane_family_corpus_counts`,
  `plunit_tests.pl:1312`. 1 of 598. Coordinator confirmed on clean `bd174725`.
- `green-all`: `rtkq-golden` (missing release extractor binary), `compile-speed`
  (baseline 4 days stale), `tsv2-test` (`hostDecode.test.ts:144`), and others.
  Re-run green-all with your diff stashed and report the delta, never the
  absolute list. ZERO legs may turn red.

## Worktree setup you will need first
`node_modules` is absent in a fresh worktree. Run `pnpm install` in `v6/tsv2`
and `v6/sprefa-store/js`. The text-door corpus is GENERATED: run
`cd v6/tsv2 && bash scripts/sweep.sh` before `parse-parity` or `text-door`.

## Known fatal
- **The cut trap.** `decl_b_column_type//3` and `host_col_type//3` differ ONLY
  in a cut. Merging them PASSES PARITY while silently widening the accepted
  language. Three agents have now caught this. Do not merge them.
- Another lane's round-3 work adds inert `cst_shape/2`, `lex_token/2`,
  `cst_extra/2`, `cst_origin/2` facts to `parse_dl_dcg.pl`. If they are present
  on your base, leave them alone; nothing under `v6/prolog/` reads them.
- Surrogate keys law: stored rels key on INTEGER ids. A composite TEXT PRIMARY
  KEY is a defect. Read `.claude/skills/sql-relational-design` and
  `.claude/skills/sqlite-costs` before touching any storage shape.
- The 10-second law: any single operation over 10s is a defect to investigate.

## Deliverable
`plans/2026-08-11-option-list-rel-generic.md`, containing:
1. The inventory table of every wrapper-peeling site you found, with file:line
   and what each peeled before and after.
2. The one walk's definition and its termination argument.
3. The fixture table, both directions, with thrown terms.
4. What fell out as a side effect, if anything.
5. Gate output verbatim, and the green-all delta against your stashed diff.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; an unbuilt construct is "TODO" or "not built
  yet". It survives only in literal code identifiers and existing filenames.
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned.
- dl variable names are descriptive, never single-letter.
- Tables and lists over prose. Numbers come from tool output only.
