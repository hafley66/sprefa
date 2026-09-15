# ibook x glm53f

Skill: `chapter-content-generator/SKILL.md` v1.10 (dmccreary/ibook-skills) with all four reference files. Model: `openrouter/z-ai/glm-5.3-flash`.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Default to Grade 10 (Senior High) if not specified." | Senior-high register throughout: 15-22 word sentences, technical terms defined in prose on first use (`effect` row, served set, `intern_snapshot`, source). |
| "Present simple concepts first, complex concepts last" | Order: `--serve` flag → when a row is written → what the row contains → loading rule → plan/code disagreements → Why → when to use → worked examples → takeaways. |
| "Every diagram, code example, or table must be preceded by prose that defines all technical terms it contains." | The mermaid diagram and the plan/code table both follow prose that defines served set, effect row, loading rule, and both sides of each disagreement. |
| "Place a level 4 markdown header before each details block" / diagram header convention | `#### Diagram: How a served goal becomes an effect row` precedes the mermaid block. |
| "Every interactive element should have clear **Learning objectives:**" with a Bloom verb | The diagram header carries "Learning objective: ... (Bloom level: Understand, verb: explain)". |
| "End with summary or key takeaways section" | `## Key takeaways` before `## What proves it`. |
| "Tables must summarize or compare information the reader already understands. Never use a table to introduce new concepts." | The plan/code table is preceded by prose defining both sides; the takeaways list summarizes stated facts. |
| "No more than 4 paragraphs of pure text without a non-text element" | No run of pure prose exceeds four paragraphs; lists, tables, the diagram, and code blocks interleave. |
| "ALWAYS place blank line before list" / "before table" | Blank lines precede every list and table. |
| "Maintain consistent voice, terminology, and visual style throughout chapter" | One voice, no first person; terms keep one name (`effect` row, served set) throughout. |
| "Verification: Always check that all concepts ... appear in generated content" | Every fact of the input page was carried into the output or logged below as dropped; citation lines were re-checked against `v8/src` before reuse. |
| "Emoji discipline" | No emoji. |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | Bare `## What` heading, diagram first | Stage-setting paragraph: what effects are for, then sections |
| heading set | What / Why / When to use / Example / What proves it | What the flag names / When a row is written / What the row contains / Reading a row / plan-code disagreements / Why / When to use / Example / Key takeaways / What proves it |
| diagram | First element, before any term is defined | After the prose that defines its terms, under a `#### Diagram:` header with a Bloom-tagged learning objective |
| citations | Inline parenthetical `path:line` beside claims | Same inline style, all ranges re-verified against `v8/src` and `plans/v8` |
| sentence length | Dense, mostly 20-35 word sentences | Senior-high target 15-22 words; condition lists broken into bullets |
| tables | One plan-vs-code table | Same plan-vs-code table plus a `## Key takeaways` list |
| length in lines | 154 | 266 |

## Dropped or changed facts

- Nothing dropped. Every fact of the input page appears in the output.
- Changed: the "What" prose sentence "Loading is an ordinary rule over `effect`, `intern_snapshot` and `cons`" is expanded into its own short section ("Reading a row with a loading rule"), keeping both clauses (ordinary rule; served goal with nothing bound is a source).
- Changed: relative links `[Not built yet](16_not_built.md)` and `[Executors](11_executors.md)` retargeted to `../../../../../16_not_built.md` and `../../../../../11_executors.md` because the output file sits five directories deeper under `book/src`. Targets unchanged.
- Changed: each dl7 example block gains a `; fixture:` annotation line above the fixture's own bytes; the remaining block bytes are verified byte-for-byte copies of `v8/fixtures/host_effect/{1_settled,3_loading,2_source}.dl7` (block 2 is the cited range `3_loading.dl7:21-26`). Same convention the input page used.
- Changed: one added sentence, "Without the flag, the served set is empty", cites `effect-demand.brief.md:23`, which is where the input page's claim ("Without the flag the served set is empty") is spelled out in the cited sources.

## Skill questions answered on the user's behalf

- Reading level: none specified in the repo; the skill's default "Grade 10 (Senior High)" was used (Step 1.4).
- Execution mode: single chapter, sequential (only one page in scope).
- Elaboration budget: the page has no `Concepts Covered` CIS table and no `learning-graph.json`; per Step 1.3, no `cis_max` exists, so per-concept budgets could not be computed. The page was treated as one concept at Tier A scale (worked examples + diagram), not a flat word count; no budget table was fabricated.
- Diagram interactivity: the skill forbids static Mermaid without `click` handlers. mdbook-mermaid does not support `click ... call` callbacks, so the diagram is kept (the input page's diagram is a fact-carrying element) and the interactivity bar is recorded as unmeetable in this book; no MicroSim or iframe substitution was made.
- Metadata frontmatter (`generated_by`, `date`): skipped; mdbook renders YAML frontmatter as page text, and the book's other pages carry none.

## Skipped as mkdocs-only (per lane brief)

- `!!!` admonitions and `<details markdown="1">` blocks (Material theme + pymdownx.details).
- MicroSims, interactive infographics, iframe sim specifications, and the MicroSim reuse search.
- `docs/course-description.md`, `learning-graph.json`, `CONTENT-GENERATION-GUIDE.md`, mascot rules: none exist in this book.
- LaTeX math rules: no equations on this page.
- Writing to `docs/chapters/NN-*/index.md` and mkdocs.yml: the book is mdbook with flat `src/NN_*.md` pages.

## Checkers run

The skill ships no checker script (no scripts directory; verification is prose checklists in the skill text). Checks actually run:

1. dl7 blocks byte-compare against fixtures:

```
b1 True
b2 True
b3 True
```

2. Citation re-verification: every `path:line` in the input page was opened and read; all ranges said what the page claims. One range was added (`effect-demand.brief.md:23`, see above); none dropped.
3. mdbook build:

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../v8/book/src/modules/../probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../v8/book/book`
```

No error. Both warnings pre-exist this lane and touch `src/probes`, not the bakeoff directory.
