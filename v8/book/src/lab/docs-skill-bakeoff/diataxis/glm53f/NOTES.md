# diataxis x glm53f

Rewrote `v8/book/src/10_effects.md` under the skill `writing-documentation` (Ferroman diataxis-documentation-skill, `SKILL.md` plus all five `references/*.md`), model `openrouter/z-ai/glm-5.3-flash` (preset `glm53f`).

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "No diagram where a list will do. A sequence, a hierarchy, a mapping and a set of conditions are all lists or tables." (SKILL.md ground rule 6) | The opening mermaid flowchart is gone; every fact it carried is now the bullet list under "What the evaluator writes". |
| "Verify before reporting. Links and facts, every time." (SKILL.md ground rule 8) | `linkcheck.py` (2 links, 0 problems) and `factdiff.py` (0 unaccounted tokens) run; output under "Checkers run". |
| "One idea per sentence. Descriptive sentences ≤ 25 words." (house-style.md, ASD-STE100) | Whole page; longest descriptive sentences are the two-sentence Application paragraph, each under 25 words. |
| "Active voice, present tense." (house-style.md) | "The evaluator takes these branches", "The code wins in every case". |
| "One approved term per concept, everywhere." (house-style.md) | "served relation", "effect row", "the outside" used consistently; no host/hosted/served alternation. |
| "Do not narrate the document's own edit history." (house-style.md) | Chris's decision and amendment 3 are stated in the present tense with their dates; no "earlier version said". |
| "Keep the reasoning, drop the framing." (house-style.md) | "The row is live interest: it exists while a rule evaluates that goal." states the rationale, not its revision trail. |
| "Surface contradictions in the source; never resolve them silently." (SKILL.md ground rule 7) | "What contradicts the plans" keeps the input's five-row plan-vs-code table. |
| "The original document keeps its path and becomes the hub." (SKILL.md ground rule 3) | N/A in this lane: the brief pins one output file; the input page is untouched. Recorded under "Skill questions answered on the user's behalf". |
| "Identify the target first." (SKILL.md ground rule 1) | Target identified as "A repository: docs/" (targets.md): relative links `[text](../../../../16_not_built.md)`, no front matter (neighbours have none), no line lint (none found), checkers run from `scripts/`. |
| "Never number sections." (SKILL.md ground rule 4) | Headings are `What the evaluator writes`, `What contradicts the plans`, `Why`, `When to use`, `Example`, `What proves it`; no `## 1.` anywhere. |
| "No hard line wrapping." (SKILL.md ground rule 5) | Each paragraph is one physical line. |
| "Every claim that came from a measurement carries the measurement." (house-style.md) | Every behavior claim carries its `src`/`tests`/`plans` line range beside it. |
| "A miss you cannot explain is a lost fact." (verification.md) | The one initial factdiff miss (`21`, from the `3_loading.dl7:21-26` citation) was fixed by restoring the range, not waved away. |

## Prev vs next

| aspect | prev (input page) | next (this page) |
|---|---|---|
| opening | mermaid flowchart with 14 nodes and edge labels | two-sentence explanation of what `effect` is and who reads it |
| heading set | `What`, `Why`, `When to use`, `Example`, `What proves it` | `What the evaluator writes`, `What contradicts the plans`, `Why`, `When to use`, `Example`, `What proves it` |
| diagram | 1 mermaid flowchart | 0 (facts it carried became a bullet list) |
| citations | 22 inline `path:line` citations | 24 inline citations; 2 moved ranges fixed (`_6_json.rs:345-366` kept, `3_loading.dl7:21-26` restored), 0 dropped |
| sentence length | mixed; several 30-40 word sentences with semicolons | one idea per sentence, ≤ 25 words, semicolons only in the contradiction table |
| tables | 2 (plan-vs-code, what-proves-it) | 2 (same tables, unchanged content) |
| length in lines | 154 | 195 |

## Dropped or changed facts

- The mermaid flowchart is dropped as a diagram. Its facts survive as the bullet list in "What the evaluator writes"; one mermaid-only detail, that `loading` consumes `effect`, `intern_snapshot` and `cons`, stays in the Why section's amendment-3 sentence and the loading fixture. Reason: skill ground rule 6.
- The dl7 example blocks now start with `; fixture: <path>` stated above the fence and are byte-for-byte copies of `v8/fixtures/host_effect/{1_settled,3_loading,2_source}.dl7` (diff-verified empty). The input's first block inlined the fixture comment as a header line instead of the file's own first line. Reason: brief law.
- Citation `_6_json.rs:345-366` re-checked: `serve_relations` starts at `:346`; the range kept as `345-366` because the doc comment on `:345` belongs to it.

## Skill questions answered on the user's behalf

- Structural question (shaping-a-set.md: "Splitting is materially different work from restructuring"): the brief pins exactly one output file, so the skill's recommended option inside that constraint is "One restructured document" (reordered into modes, nothing to relink). Chosen; no split.
- Audience question (worked-example.md, target-state procedures): the subject is built and tested, so the "proposal" branch does not apply; no status callout needed.

## Checkers run

Skill scripts (from `skills/writing-documentation/scripts/`):

```
$ python3 linkcheck.py v8/book/src/lab/docs-skill-bakeoff/diataxis/glm53f/10_effects.md

2 links checked, 0 problem(s)

$ python3 factdiff.py v8/book/src/10_effects.md v8/book/src/lab/docs-skill-bakeoff/diataxis/glm53f/10_effects.md
inline-code tokens: 54 in original, 0 not found in the new set
@logins and teams: 0 in original, 0 not found in the new set
wikilink targets: 0 in original, 0 not found in the new set
numbers: 31 in original, 0 not found in the new set

0 unaccounted token(s)
```

Manual checks (verification.md "By hand"): dl7 fences diff-identical to their fixture files; no numbered headings; `grep -c 'mermaid'` on the output is 0; both cited sibling pages exist at `../../../../16_not_built.md` and `../../../../11_executors.md`.

Build:

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../probes/21_partial_in_field.dl7`   (pre-existing, untouched by this lane)
 INFO Running the html backend
 INFO HTML book written to .../v8/book/book
```
