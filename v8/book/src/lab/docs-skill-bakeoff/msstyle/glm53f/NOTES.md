# msstyle x glm53f

Skill: `vendor-claude-ai-technical-writing-skill/SKILL.md` (Microsoft Manual of Style skill). Model: openrouter/z-ai/glm-5.3-flash.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Write to the reader, not about them. Use imperative voice for instructions ... Use active voice for explanations." | "Use `effect` when...", "Do not use `effect` when..."; "Each example runs the real binary through `book/show.sh`." |
| "Keep it human. Short sentences. Plain words." | Every explanatory paragraph is one to three short sentences; no compound narration. |
| "Avoid time-stamped language outside release notes." | Prose carries no "currently"/"now"; design dates appear only inside the quoted decision record, which is a fact, not prose. |
| "Use **Title Case** ... Do not skip levels" | Headings: "What an Effect Row Is", "Plan Versus Code"; H1 then H2, no level skips. |
| "Bullet lists for unordered items; numbered lists when sequence matters" | "How the Evaluator Writes the Row" and "Deciding When to Use Effects" are bullets; no fake sequences. |
| "Parallel structure throughout ... Capitalize the first word of each item" | All three bullets in "Deciding When to Use Effects" open with a capitalized verb phrase; the don't-list parallels it. |
| "Introduce every table with a sentence before it" | "Plan Versus Code" ("Two plans preceded the built behavior..."), "What Proves It" ("The table lists..."). |
| "No punctuation in column headers" / "Keep cells short" | Table headers `plan`, `code`, `winner`, `claim`, `path`, `command` carry no punctuation. |
| "Concept (what and why) → Task (how) → Reference" | Page order: concept sections, then usage decision, then examples, then the proofs table. |
| "Keep the overview to two sentences." | Opening paragraph after H1 is two sentences. |
| "Use **see**, not 'refer to'" | "see [Not built yet](16_not_built.md)", "see [Executors](11_executors.md)". |
| "Link text must describe the destination" | Link text is the chapter name; no "click here". |
| "Spell out zero through nine; numerals for 10 and above ... Always use numerals for measurements" | "one row", "Two plans"; measurements stay numerals: "arity 2", "keys `[0,1]`", "exit 1". |
| "Never start a sentence with a numeral" | No sentence opens with a numeral. |
| "Define acronyms on first use" | No acronyms are used; `effect`, `intern_snapshot`, `cons` are code identifiers and stay in backticks. |
| "**Ask before you change anything.**" (adapted: brief forbids asking) | Choice recorded below; the page is written without interactive sign-off. |
| "Short, accurate docs are valued over long, comprehensive-looking ones" | No padding sections; the SSG-hygiene sections of the page template (Prerequisites, Troubleshooting) are omitted because a language chapter has no procedure to preface. |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | Headed `## What` that opens with a bare mermaid flowchart, prose after | Two-sentence prose opening under H1, then concept sections |
| heading set | `What`, `Why`, `When to use`, `Example`, `What proves it` | Title-Case gerund-style set: "What an Effect Row Is", "Serving a Relation From Outside", "How the Evaluator Writes the Row", "Plan Versus Code", "Design Decisions", "Deciding When to Use Effects", "Examples" (H3 per fixture), "What Proves It" |
| diagram | Same mermaid, no caption, first thing on the page | Same mermaid, one caption sentence tying the flow to a round; diagram moved after the opening |
| citations | Inline parenthetical citations inside flowing prose | Same citations, placed beside each bullet/claim; none dropped |
| sentence length | Several long multi-clause sentences (e.g. line 19, 36) | Split into shorter sentences; one clause per fact where possible |
| tables | Two tables dropped in with no introduction | Same two tables, each introduced by a sentence |
| length in lines | 154 | 183 |

## Dropped or changed facts

- None dropped. Every input fact, plan-dispute row, fixture, console transcript, and citation appears in the rewrite.
- Changed framing only: the page template's Prerequisites/Troubleshooting/Related Topics sections were skipped (no UI procedure in a language chapter); "Why" became "Design Decisions"; the H3 sub-headings in Examples name the scenario.
- Added sentence: the caption under the mermaid diagram restates what the diagram's edges already encode (each edge is verified against the same code the input cites); no new behavior claim.
- Added sentence: "Each example runs the real binary through `book/show.sh`." Verifiable from `book/show.sh` itself.

## Skill questions answered on the user's behalf

- The four setup questions (doc root, SSG, UI config, feature map): doc root `v8/book/src`, SSG mdbook, UI config none (a compiler book has no UI), feature map `v8/src` phases to numbered chapters. The skill recommends confirming per-project setup; the bakeoff brief forbids asking.
- "Are you looking to create, edit, validate...?" → create (the brief asks for a rewrite of the page under a new path).
- The skill's create workflow `workflows/create.md` does not exist in the vendor repo (only `workflows/coverage.md` ships). Fell back to the page template and pattern in `reference/syntax.md` plus `reference/style-rules.md`. The skill's README describes what create.md would contain (requirements gathering, topic placement, front matter, Concept→Task→Reference, validation at the end); those parts were applied from the README's description.
- Heading capitalization choice: Title Case, the style-rules default.

## Checkers run

The skill ships no checker script. Its grep patterns (`reference/ui-terms.md`) were run against the output:

```
$ rg -i "(?i)\bclick on\b|(?i)\bdeselect\b|(?i)\bdropdown\b|(?i)\blaunch\b|(?i)\bhighlight\b|(?i)\binvoke\b|(?i)\bthe user\b|(?i)\b(we|our|us)\b|(?i)\brefer to\b|(?i)\bclick here\b" 10_effects.md
(no matches)
```

No matches.

`mdbook build`:

```
$ cd v8/book && mdbook build >/dev/null 2>&1; echo "mdbook exit=$?"
mdbook exit=0
```

Build exits 0. The preprocessor emits ERROR/WARN lines about missing `{{#include ../probes/*.dl7}}` files under `book/src/modules/` (for example `20_full_application_bind.dl7`); those files are absent from this branch and the errors predate this lane's output, which lives under `book/src/lab/` and introduces no include. The project ships no lint command; that phase was skipped per the brief.
