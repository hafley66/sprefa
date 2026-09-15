# toss x glm53f

Skill: `technical-writing` (Toss guide, gwagjiug), entry file `/Users/chrishafley/projects/claude-research/skills_archive/vendor-technical-writing/skills/technical-writing/SKILL.md` plus all six `references/` files and the `explanation-doc.md` template. Model: `openrouter/z-ai/glm-5.3-flash`.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Identify the audience, task, and document type before editing" | Explanation type chosen; the reader learns the mental model of `effect`. Choice recorded under "Skill questions" below. |
| "Lead with reader value before implementation details" (`structure-principles.md`) | Overview opens with what `effect` does and ends with what the reader can do after the page; history and plan-vs-code table sit in Background. |
| "Explain the concept and why it exists ... background and constraints ... mechanism" (`document-types.md` explanation) | Sections Overview, Background, How it works, Tradeoffs, Example, Verification, Related. |
| "Include the core keyword in the heading. ... plain, declarative, action-oriented headings" (`structure-principles.md`) | `# Effects`, `## How it works`, `### Serving`, `### When dl8 writes an effect row`, `### The application term`, `### Reading effect rows`. |
| "An overview should set expectations and help readers decide whether to continue" (`structure-principles.md`) | Overview's second paragraph states the three skills the page teaches. |
| "Explain new concepts when they first appear" (`structure-principles.md`) | `Application`, `none`, source, loading rule each defined at first use in How it works before Example uses them. |
| "Make the actor clear" / active voice (`sentence-style.md`) | "dl8 writes no `effect` row", "a rule can open the argument list", "every evaluation ... writes one row". |
| "Remove meta-discourse" (`sentence-style.md`) | No "we", no process narration; sentences state behavior directly. |
| "exact commands, concrete inputs and outputs" (`sentence-style.md`) | Console blocks kept verbatim with `exit` lines; fixture file named above each dl7 block. |
| "Preserve code blocks, commands, API identifiers ... warnings, error messages" (SKILL.md guardrails) | All dl7 blocks are byte-identical fixture copies; console output, flags, and diagnostic names unchanged. |
| "Do not invent missing technical facts. Mark gaps as questions or TODOs" (SKILL.md) | No new dl8 claims; every claim carries the input page's citation, re-verified against `v8/src`. |
| "Link from ... tutorials to references" cross-linking (`structure-principles.md`) | `## Related` links `11_executors.md` and `16_not_built.md`. |
| "Concrete examples or diagrams when useful" (`document-types.md`) | The input mermaid flowchart kept in Overview beside the prose description. |

## Prev vs next

| aspect | prev (input page) | next (this page) |
|---|---|---|
| opening | `## What` heading followed immediately by a mermaid flowchart | Overview prose: one-sentence concept, what `--serve` does, what the reader can do after the page |
| heading set | What / Why / When to use / Example / What proves it | Overview / Background / How it works (4 subsections) / Tradeoffs / Example (4 subsections) / Verification / Related |
| diagram | mermaid at top, no prose around it | same mermaid, kept under Overview with the concept stated in prose beside it |
| citations | inline `path:line` after each sentence, one dense paragraph per mechanism | same citations kept per claim, spread across short How-it-works subsections; loading fixture range fixed `21-26` to `22-27` |
| sentence length | multi-clause sentences with parenthetical citation stacks | shorter sentences; each clause that carries a citation is its own sentence where it reads cleanly |
| tables | 2 (plan-vs-code, what-proves-it) | same 2 tables, plus none added |
| length in lines | 154 | 186 |

## Dropped or changed facts

- none dropped; every input fact survives.
- changed: the loading fixture citation range `3_loading.dl7:21-26` moved to `:22-27`, where the block actually sits in the file.

## Skill questions answered on the user's behalf

- Document type: the input page explains a mechanism and its tradeoffs, so `document-types.md`'s Explanation type and `explanation-doc.md` shape (Overview, Background, How It Works, Tradeoffs, Example, Related) were chosen over reference or tutorial.

## Checkers run

The skill ships none. Verification performed: every `path:line` citation opened and read; dl7 blocks diffed against fixture files; `mdbook build` output below.

`mdbook build` output:

```
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to .../v8/book/book
```

`mdbook build` also prints 3 `ERROR` lines for `{{#include ../probes/20_full_application_bind.dl7}}`, `21_partial_in_field.dl7`, `22_curry_prelude_name.dl7` from the modules chapter. Baseline check with this lane's output stashed prints the same 3 errors, so they are pre-existing on this branch (the files exist in the main worktree under `v8/book/src/probes/`) and are not caused by this page. No error involves `lab/docs-skill-bakeoff/`.
