# BRIEF: 10 tests that break us

## Base
- Worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `bf157562` (origin/main). FIRST action: `git log --oneline -1`.
  Any other base = STOP AND REPORT.

## One sentence
Find ten tests that break this system, run each one, and come back with the
verbatim failure plus the throw site, so the user reads a list of real defects
instead of a list of worries.

## The user's word (2026-08-12)
"find 10 tests that stretch and exacerbate the current system, i.e. where our
shit breaks."

A "break" is one of these, in decreasing order of value:
1. **Wrong answer, no error.** The system accepts a program and returns the
   wrong rows. Highest value by a wide margin.
2. **Crash or hang.** Including anything over 10 seconds; that is the
   10-second law and it makes the case a defect on its own.
3. **A compiler error for something the language should accept.** Only counts
   if you trace it to the throw site and say whether it encodes a real
   impossibility or unfinished work.

A program correctly rejected with a clear message is NOT a break. Do not pad
the list with those.

## You do not fix anything
Report only. Three live lanes are editing the compiler right now and a fix from
you would collide. If you find a one-line fix, write the fix as a diff INSIDE
the plan doc and leave the tree alone.

If a break turns out to be a language or type-system design question, it comes
back as a fork with the throw site cited. The user rules on language design;
you do not, and neither does the plan doc.

## Files you own
| path | permission |
|---|---|
| `plans/2026-08-12-ten-tests-that-break-us.PLAN.md` | create |
| `plans/2026-08-12-ten-tests-that-break-us.PLAN.visual.human.unga.md` | create |
| `v6/prolog/labs/break-hunt/**` | create, this is where every reproducing `.dl6` lives |

Everything else in the repo is READ-ONLY to you. Zero other paths may appear in
`git status`.

## Leads, not answers
Each row below is a hypothesis someone recorded. A lead you DISPROVE by running
it is a real finding and belongs in the doc as a disproven row. A lead you
confirm needs the failure text, not the hypothesis restated.

| # | lead | where it was recorded |
|---|---|---|
| 1 | a DERIVED rel wired as a reference target makes it an arrival target too, and the oracle returns a DUPLICATED row with zero errors: `[grade_tag(401,ripe),grade_tag(401,ripe)]` | `CLAUDE.md`, measured 2026-08-08, no error anywhere in `analyze.pl` or `0_program_check.pl` |
| 2 | `bool` is a declarable column type with ZERO operators | `v6/prolog/0_type_plane.pl:127` |
| 3 | `text` has no concat, split, or substr SCALAR; `group_concat` is an aggregate | `v6/prolog/compile/registry.pl:174-178` |
| 4 | mutual recursion between two rels | briefs `mutual-recursion-*`, `dd-mutual-recursion-research` |
| 5 | recursion over a CYCLIC graph, not a DAG | `Family::Cycle` just landed in `v6/labs/exec_shootout/harness` |
| 6 | the swipl oracle walls before 10k rows (s1/1k = 1.4s vs tsv2 33ms) | `v6/bench-cli/CONTRACT.md` section 7 |
| 7 | float / REAL and `avg` | roadmap phase 5, `plans/2026-07-29-v6-alpha-golden-plan.md` |
| 8 | JSON null as the `none` atom, landed hours ago in PR #196 | `v6/prolog/conformance/fixtures/json_null_is_none.pl` |
| 9 | `option(list(_))` and nested option types | `plans/2026-08-11-option-list-rel-generic.md` |
| 10 | empty relations, empty aggregates, aggregate over zero rows, retraction of the last row | nobody recorded this; that is why it is here |
| 11 | a rel that joins itself, a self-loop edge, a rel with 15+ columns | unmeasured |
| 12 | deep negation: `not()` over a recursive rel, `not()` over an aggregate | unmeasured |

Twelve leads, ten slots. Rank by what actually broke, and say which leads you
ran and found solid. A lead you never ran is reported as never run.

## Method, per candidate
1. Write the smallest `.dl6` that shows it, in `v6/prolog/labs/break-hunt/`.
2. Compile and run it. The doors:
   ```bash
   cd v6/tsv2 && bash scripts/sweep.sh          # regenerates the manifest
   cd v6 && just green-all                      # the full gate, for reference
   ```
   `v6/prolog/compile/out/manifest.json` carries `bucket` + `reason` per
   fixture and is the authority on what compiles. Grep it BEFORE claiming a
   construct does not compile. A header comment is not the language.
3. Capture the failure VERBATIM. Wrong rows: show expected next to actual.
4. Trace it. `grep` for the message text, find the `throw` or the arm that
   produced it, cite `file:line`.
5. Answer one question per case: is this a real impossibility, or work that was
   never finished? Most named errors in this repo were decided by an agent with
   nothing measured. An error message is a hypothesis, never an edict.

## Deliverable
`plans/2026-08-12-ten-tests-that-break-us.PLAN.md`, in this order:

1. TOC.
2. The ranked table: rank, one-line title, break class (wrong-answer / crash /
   error-for-valid-program), the `.dl6` filename, the throw site `file:line`.
3. One section per case, all ten, each containing: the `.dl6` in a fenced
   block, the exact command, the verbatim output, expected versus actual when
   the answer is wrong, the throw site with 3 lines of surrounding code, and
   the one-sentence verdict on impossible-versus-unfinished.
4. The leads you ran and found SOLID, with the command that proved it.
5. The leads you never ran, named.
6. Forks for the user: every case whose fix is a language or type-system
   design call, written as a fork with options, no recommendation chosen.

Plus `plans/2026-08-12-ten-tests-that-break-us.PLAN.visual.human.unga.md`:
plain words, ascii or mermaid diagrams, ZERO citations, for a reader with no
context. A plan without this second doc is undelivered.

## Anti-cheat
| rule | why |
|---|---|
| Every claimed break carries the command AND its verbatim output | a break without output is a worry |
| Every `.dl6` you cite is committed in `v6/prolog/labs/break-hunt/` | so the coordinator re-runs it, not re-reads about it |
| Grep `manifest.json` before saying a construct does not compile | headers in this repo have been wrong four times about their own grammar |
| No case is reported from reading code alone | reading is a hypothesis, running is a finding |
| A correctly-rejected program is not a break | padding the list wastes the user's morning |
| You do not edit the compiler, the runtime, or any fixture outside your lab dir | three lanes are editing them right now |

## Worktree setup, before your first commit
```bash
mkdir -p v6/sprefa-extract/target/release
cp /Users/chrishafley/projects/sprefa/v6/sprefa-extract/target/release/extract \
   v6/sprefa-extract/target/release/extract
(cd v6/tsv2 && pnpm install)
(cd v6/sprefa-store/js && pnpm install)
```
The pre-commit comment rail needs all four. `git commit -n` and `--no-verify`
are FORBIDDEN; a blocked commit is a finding, not an obstacle to route around.

## Rails
- The 10-second law: any single command over 10 seconds is itself a defect to
  record. Named exception: SCIP indexing.
- Commit after each case, with the case number in the message.
- Never spawn a subagent.

## Style laws, inline so you need no judgment
- No em dashes.
- Banned words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`. Use source, base layer, critical, mode.
- The word "refusal" is banned in prose; a compiler error for an unbuilt
  construct is "TODO" or "not built yet". It stays only in code identifiers.
- No `here is`, `here's`, `below is`, `the following`. The content just starts.
- No `clearly`, `obviously`, `as expected`, `in reality`, `honestly`.
- Comments state only constraints the code cannot show.
- Tables and lists over prose. Docs open with a TOC.
- dl variable names are descriptive, never single-letter.
