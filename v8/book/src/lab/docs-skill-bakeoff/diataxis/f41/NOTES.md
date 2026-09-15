# diataxis x f41

Skill: `skills_archive/vendor-diataxis-documentation-skill/skills/writing-documentation/SKILL.md` and every file it names (`references/targets.md`, `shaping-a-set.md`, `house-style.md`, `verification.md`, `worked-example.md`, `scripts/linkcheck.py`, `factdiff.py`, `unwrap.py`). Model: `openrouter/deepseek/deepseek-v4.1-flash` (preset `f41`).

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Identify the target first, then read its section of references/targets.md." | Target is a repository (an mdbook book in a git repo). Read the repository section: relative-path links, no front matter, no line-width linter found. Links written as `../../../../16_not_built.md` so they resolve from `f41/`. |
| "If the project has its own documentation conventions ... those win. This skill fills the gaps and never overrides them." | Kept the book's chapter heading set (`## What`, `## Why`, `## When to use`, `## Example`, `## What proves it`), the same shape as `11_executors.md`. |
| "Never number sections. No `## 1. Foo`, no `§4` cross-references." | All headings are unnumbered. Cross-references are links to named pages: `[Executors](../../../../11_executors.md)`. |
| "No hard line wrapping ... One paragraph is one physical line." | Every paragraph is one physical line. `unwrap.py` is idempotent on the file (166 -> 166, no change). |
| "No diagram where a list will do." | The input's mermaid flowchart is replaced by a six-item bullet list. Zero ```mermaid blocks remain. |
| "Surface contradictions in the source; never resolve them silently." | The five-row plan-vs-code disagreement table is kept. The stale fixture comment is flagged as an open question for the maintainer. |
| "Do not narrate the document's own edit history." | Dropped "Amendment 3, same day" and "which was the first design ... before amendment 3". The facts those phrases carried survive as present-tense statements. |
| "One idea per sentence. Descriptive sentences ≤ 25 words, instructions ≤ 20." | The input's single dense `## What` paragraph became six bullets, one idea each. |
| "Active voice, present tense." | "The evaluator resolves each name against the program's declared names"; "the code writes a row for every evaluation". |
| "One approved term per concept, everywhere." | "served relation", "effect row", "evaluation", "executor" used one way throughout. |
| "Paragraphs ≤ 6 sentences." | Every paragraph is one or two sentences. |
| "Prefer the number to the adjective." | "Two goals, one settled and one not"; "arity 2"; "lines 21-26". |
| "Every claim that came from a measurement carries the measurement." | Every behavioural claim keeps its `path:line` citation; the console transcripts keep their exit codes. |
| "Verify: scripts/linkcheck.py, then scripts/factdiff.py. Fix, re-run." | Both run clean; see Checkers run. |
| "Ask the structural question (ground rule 2)." | Answered on the user's behalf; see below. |
| "duplicate freely between documents" / "Generate it from a data table" | N/A. No per-item reference set, no second document. Duplication was not needed. |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | `## What` opens with a mermaid flowchart, no prose until after it | opens with one sentence defining the mechanism, then a six-item list |
| heading set | `# Effects`; `## What`, `## Why`, `## When to use`, `## Example`, `## What proves it` | unchanged; the repo convention wins per the skill |
| diagram | 1 mermaid flowchart | 0 diagrams; the flowchart's branches became the six bullets |
| citations | inline `path:line` spans, dense in one paragraph | the same spans, each attached to the sentence it proves |
| sentence length | multi-clause sentences of 40-90 words | one idea per sentence, <=25 words |
| tables | 2 (plan-vs-code, `What proves it`) | 2, same rows; one lead line added above the plan-vs-code table |
| length in lines | 154 | 166 |
| dl7 blocks | 1 whole file, 1 range (`3_loading.dl7:21-26`) | 3 whole files, each byte-for-byte, path named above the fence |
| mermaid blocks | 1 | 0 |

## Dropped or changed facts

- Dropped "Amendment 3, same day" and "which was the first design (`effect-demand.brief.md:47`) before amendment 3": edit-history narration, barred by `house-style.md` ("Do not narrate the document's own edit history"). The facts survive: the row is live interest (in `## Why`), the fixture comment reflects an earlier plan (in the Example open question), and `effect-demand.brief.md:47` still appears there.
- Changed the loading dl7 block from the range `3_loading.dl7:21-26` to the whole `3_loading.dl7` file, byte-for-byte, with "lines 21-26" named in the sentence above. Reason: the hard law that a dl7 block must be a byte-for-byte copy of a fixture file.
- Changed the `1_settled` and `2_source` dl7 blocks: the input's `; fixture: ...` marker line was replaced by a path line above the fence. Reason: same hard law; the fence body is now exactly the file bytes.
- Removed the mermaid flowchart. Reason: skill ground rule 6; its branches survive as the six bullets and the `## Why` paragraph.
- Added one lead line, "Five places where a plan document and the code disagree:", and one-sentence introductions to each Example fixture. Both are derived from the table and fixtures; no new behaviour claim.
- Added an open question flagging the stale comment on line 1 of `fixtures/host_effect/1_settled.dl7`. Reason: ground rule 7.
- No other facts dropped. `factdiff.py` reports 0 of 54 inline-code tokens, 0 of 31 numbers, and 0 wikilink targets missing.

## Skill questions answered on the user's behalf

- Structural question: the skill recommends **folder + hub**. The lane pins one output file at `.../f41/10_effects.md` and forbids editing `SUMMARY.md`, so a split is not deliverable. Chose the second option, **one restructured document**, ordered explanation -> how-to -> reference in place. Recorded here as the skill directs.
- Proposal how-tos now or deferred: not applicable; the subject is built and tested.
- `targets.md` "Anything else" (link resolution, placement): answered by repo inspection rather than asking. Links resolve by relative path; the neighbours set the heading convention; no front matter.

## Checkers run

```console
$ python3 scripts/linkcheck.py --root v8/book/src v8/book/src/lab/docs-skill-bakeoff/diataxis/f41/10_effects.md

2 links checked, 0 problem(s)
```

```console
$ python3 scripts/factdiff.py v8/book/src/10_effects.md v8/book/src/lab/docs-skill-bakeoff/diataxis/f41/10_effects.md
inline-code tokens: 54 in original, 0 not found in the new set
@logins and teams: 0 in original, 0 not found in the new set
wikilink targets: 0 in original, 0 not found in the new set
numbers: 31 in original, 0 not found in the new set

0 unaccounted token(s)
```

```console
$ python3 scripts/unwrap.py /tmp/f41_unwrap.md
  f41_unwrap.md: 166 -> 166 lines
$ diff -q /tmp/f41_unwrap.md v8/book/src/lab/docs-skill-bakeoff/diataxis/f41/10_effects.md
(idempotent; no change)
```

Byte check of every dl7 block against its fixture (extracted each fence, compared to the file):

```console
fixtures/host_effect/1_settled.dl7 EXACT
fixtures/host_effect/3_loading.dl7 EXACT
fixtures/host_effect/2_source.dl7 EXACT
```

Book build:

```console
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../src/modules/../probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../v8/book/book`
$ echo $?
0
```

Three `ERROR updating "{{#include ../probes/*.dl7}}"` lines appear in `modules/` pages (`20_full_application_bind.dl7`, `21_partial_in_field.dl7`, `22_curry_prelude_name.dl7`), for probe files absent from the repo. They appear identically when the lab directory is moved aside and the base is rebuilt, so none come from this page. The build exits 0.
