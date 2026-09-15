# ibook x f41

Skill: `chapter-content-generator` (`vendor-ibook-skills/skills/chapter-content-generator/SKILL.md`) plus its `references/{content-element-types,reading-levels,blooms-taxonomy,math-equations}.md` and `book-installer/references/mascot-placement-rules.md`. Model: `f41` (openrouter/deepseek/deepseek-v4.1-flash).

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Start with introductory paragraphs connecting to chapter summary" (Step 2.4, content structure) | the two paragraphs under `# Effects` before `## What` |
| "Present concepts in pedagogical order (simple to complex)" (Step 2.4 principle 2) | served set and application term defined first; `effect` kernel status next; loading rule last |
| "Vocabulary before visuals: Every diagram, code example, or table must be preceded by prose that defines all technical terms it contains." (Step 2.4 principle 3) | served set, application term, positive/negative goal, aggregate body, `intern_snapshot`, `cons` all defined before the mermaid, the tables, and the loading block |
| "Bridge sentences before code: Before any code example, include a plain-language sentence explaining what the code does and what its key parameters mean." (Step 2.4 principle 3) | "The first fixture serves `fetch_json` and supplies one settled row."; "A rule opens the application with `intern_snapshot` and `cons`."; "A goal with nothing bound makes the whole pattern free" |
| "Prose first, tables reinforce: Tables summarize or compare information the reader already understands. Never use a table to introduce new concepts." (Step 2.4 principle 3) | the plan-vs-code table is introduced by "The rules above settle five earlier disagreements..." |
| "Signpost what's coming: Before complex elements, add a navigation cue" (Step 2.4 principle 3) | "Before the diagram, three terms need definitions..."; "The diagram traces one pass of evaluation..." |
| "Goal: No more than 4 paragraphs of pure text without a non-text element." (Step 2.4 principle 5) | every prose run is followed by a bulleted list, the diagram, a table, a `dl7` block, or a `console` block |
| "Use diverse element types (don't repeat the same type)." (Step 2.4 principle 5) | prose, bulleted lists, mermaid, two markdown tables, three `dl7` blocks, four `console` blocks |
| "End with summary or key takeaways section" (Step 2.4, content structure) | `## Key takeaways` |
| "Default to Grade 10 (Senior High) if not specified." (Step 1.4) | measured 15.2-word average prose sentence, mixed simple and compound (see Prev vs next) |
| "Reference a section of the 2001 Bloom Taxonomy when you describe a learning objective" (Step 2.4, interactive elements emphasis) | the caption "*Learning objective (Understand): ...*" under the diagram |
| "ALWAYS put blank line before list" / "ALWAYS put blank line before table" (Step 2.4, element types) | blank line before every `-` list (lines 62, 68) and both tables (lines 45, 170) |
| "Do NOT use dollar sign delimiters (`$` or `$$`)" (Step 2.4 principle 5) | no equations appear; no `$` delimiters |
| "Emoji discipline: ... leave them out." (Best Practices 14) | no emoji |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | `# Effects` then straight into `## What` with the mermaid diagram | two framing paragraphs define the effect row and name the three terms that follow |
| heading set | `# Effects`, `## What`, `## Why`, `## When to use`, `## Example`, `## What proves it` (6 headings) | same six plus `## Key takeaways` (7 headings); `## What` now opens with definitions and bullets |
| diagram | mermaid flowchart first, no caption, before any prose | same mermaid flowchart, moved below the term definitions, with a bridge sentence and a Bloom learning-objective caption |
| citations | inline `path:line` throughout; two tables; two relative cross-links | all inline citations preserved; both cross-links (`16_not_built.md`, `11_executors.md`) preserved |
| sentence length | 17 prose sentences, 16.0 word average | 41 prose sentences, 15.2 word average (Senior High band) |
| tables | 2 (plan-vs-code, proves) | 2 (same content, each introduced by bridging prose and preceded by a blank line) |
| length in lines | 154 | 183 |

## Dropped or changed facts

- None dropped. Every claim, citation, table row, console block, and the mermaid graph survive; all cited ranges were re-opened and match the page.
- Changed: an opening paragraph and `## Key takeaways` were added (framing, no new behavioral claim).
- Changed: the mermaid diagram moved below the term definitions to satisfy the define-before-display rule.
- Changed: a few short sentences were merged or extended to reach the skill's Senior High reading level.
- Changed: the `dl7` blocks omit the repo's in-fence `; fixture: <path>` marker line. The brief asks to "say which file above the block", so each file (and the `3_loading.dl7:21-26` range) is named in prose above its block, and the block bytes equal the fixture exactly. Verified by the checker below.
- Added citation `tests/_14_host_effect.rs:1-5` to define the reified program JSON node in the diagram; it is the test header that states `dl8 compile` output feeds `dl8 eval`.

## Skill questions answered on the user's behalf

- Reading level not specified: the skill says "Default to Grade 10 (Senior High) if not specified", so Senior High was chosen.
- Execution mode: the skill defaults to Sequential and single-chapter processing; one page, no parallel agents.
- Mascot: the skill places mascots only "when a mascot is defined" in `CONTENT-GENERATION-GUIDE.md`; this book defines none, so no mascot admonition was written (mascot rules were read but not exercised).
- MicroSim reuse check: the skill's availability probe points at `/Users/dan/Documents/ws/search-microsims`; absent here, so the skill's own "graceful degradation" path applies and the reuse step was skipped.
- "Do NOT use this skill when ... Content already exists and just needs editing (use Edit tool directly)": overridden by the bakeoff brief, which mandates a skill-driven rewrite.

### Skips

- MicroSims and the `<details markdown="1">` specification blocks: skipped. The brief says skip MicroSims; the book is mdbook and has no sim harness.
- MkDocs admonitions (`!!!`, `???`): skipped. The brief marks them mkdocs-only; mdbook does not render them.
- `mkdocs.yml` edits: skipped. The book builds with `book.toml`.
- Interactive-element rule ("Every Visual Element Must Be Interactive"; "Mermaid diagrams are acceptable ONLY when every node has a `click` directive"): not met. mdbook-mermaid has no infobox callback, so a `click` directive would produce no feedback; the diagram is a static flowchart and its interactivity rule is recorded unmet rather than faked.

## Checkers run

The skill ships no checker script. Its references are prose; the mascot validator lives in `book-installer`, not this skill, and no mascot is used, so it is not applicable.

Fixture byte check (block content vs fixture bytes):

```
$ python3 - <<'EOF'
import re
p="book/src/lab/docs-skill-bakeoff/ibook/f41/10_effects.md"
blocks=re.findall(r"```dl7\n(.*?)```",open(p).read(),re.S)
files=["fixtures/host_effect/1_settled.dl7",("fixtures/host_effect/3_loading.dl7",21,26),"fixtures/host_effect/2_source.dl7"]
for b,f in zip(blocks,files):
    src=("".join(open(f[0]).readlines()[f[1]-1:f[2]])) if isinstance(f,tuple) else open(f).read()
    print(("%s:%s-%s"%(f[0],f[1],f[2])) if isinstance(f,tuple) else f, "MATCH" if b==src else "DIFF")
EOF
fixtures/host_effect/1_settled.dl7 MATCH
fixtures/host_effect/3_loading.dl7:21-26 MATCH
fixtures/host_effect/2_source.dl7 MATCH
```

Build, as the brief directs (`cd v8/book && mdbook build 2>&1 | tail -5`). Exit code 0. The page is not in `SUMMARY.md` (forbidden), so the book build does not render it; the three `ERROR` lines are pre-existing missing `{{#include}}` probes in `modules/*.md`, untouched by this lane:

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../v8/book/src/modules/../probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../v8/book/book`
$ grep ERROR build.log | grep -v probes
(none)
```

Because the page is unlisted, it was also built in isolation (temporary `SUMMARY.md` outside the repo) to prove it renders and the mermaid preprocessor accepts it:

```
$ mdbook build   # temp book, src/SUMMARY.md -> 10_effects.md
 INFO Book building has started
 INFO Running the html backend
 INFO HTML book written to `.../f41book/book`
$ ls book/10_effects.html
book/10_effects.html
```
