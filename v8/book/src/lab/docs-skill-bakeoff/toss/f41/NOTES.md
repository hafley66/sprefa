# toss x f41

Skill `technical-writing` (Toss guide, `gwagjiug/technical-writing`), entry file `skills/technical-writing/SKILL.md`, applied with model `openrouter/deepseek/deepseek-v4.1-flash` (preset `f41`). Output: `10_effects.md` in this directory. Document type chosen: Explanation.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Identify the audience, task, and document type before editing." (SKILL.md Core Workflow 1) | Explanation chosen from `references/document-types.md`; the reader is learning dl8, not the author. |
| "Select the structure from `references/document-types.md` and the matching template from `assets/templates/`." (Workflow 2) | Section set follows `assets/templates/explanation-doc.md`: Overview, Background, How It Works, Tradeoffs, Example, Related. |
| "Apply the information architecture principles in `references/structure-principles.md`: one topic per page, value first, effective headings, overview, predictable order, and sufficient background." (Workflow 3) | One H1; no H4; Overview before Background; Explanation order concept -> context -> mechanism -> tradeoffs -> examples. |
| "Polish sentences with `references/sentence-style.md`." (Workflow 4) | Short declarative sentences; actor named ("The evaluator writes...", "The driver resolves..."). |
| "Preserve code blocks, commands, API identifiers, parameter names, version numbers, warnings, error messages, dates, metrics, and product behavior unless the user explicitly asks to change them." (Guardrails) | `dl7` blocks are byte-for-byte fixture copies; console commands and diagnostics unchanged. |
| "Do not invent missing technical facts." (Guardrails) | Every behavior claim cites a `v8/src` range or a `v8/fixtures` file; no claim added beyond the input. |
| "Prefer concrete examples, conditions, prerequisites, and expected results over vague benefits." (Guardrails) | Fixture programs, `--serve` conditions, and expected console exits. |
| "Keep headings predictable and keyword-rich." (Guardrails) | Overview, Background, How it works, Design decisions, Tradeoffs, Example, What proves it, Related. |
| "Put the reader's value or outcome before implementation details." (Guardrails) | Overview states what an effect row buys the reader before any code range. |
| Explanation "Require: the concept and why it exists, ... mechanism or architecture, tradeoffs and failure modes, concrete examples or diagrams when useful." (`document-types.md`) | Overview (concept), Background (why), How it works (mechanism + diagram), Tradeoffs (failure modes), Example. |
| Explanation "Avoid: turning the explanation into a step-by-step tutorial." (`document-types.md`) | No numbered steps; sections are conceptual. |
| "Compactness: ... remove meta-discourse." (`sentence-style.md`) | Dropped the "Chris, 2026-09-14:" attribution and the input's `; fixture:` narration. |
| "Do not remove warnings, assumptions, prerequisites, or edge cases just to make the page shorter." (`sentence-style.md`) | Kept the do-not-use conditions, the never-retracted limit, and the unknown-name diagnostic. |
| "Concreteness: Make actions, conditions, and results observable. Prefer: exact commands, concrete inputs and outputs." (`sentence-style.md`) | `bash book/show.sh` lines with their printed rows and `exit` codes. |
| "Terminology Consistency: Use one term for one concept." (`sentence-style.md`) | "served relation", "effect row", "intern", "application" used throughout. |
| "Code And Identifier Safety: Never polish inside code blocks unless the user asks." (`sentence-style.md`) | Fixtures copied verbatim; console blocks verbatim. |
| "Cross-Linking: ... Link from tutorials to references after the guided path." (`structure-principles.md`) | `## Related` links Executors and Not built yet after the examples. |
| "For rewrites: mention preserved constraints and any unresolved factual gaps." (SKILL.md Output Defaults) | The "Dropped or changed facts" section records what moved and what stands; no unresolved gap remains. |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | `## What` then a `mermaid` block; no prose lead. | `## Overview` paragraph stating what an effect row is and what the reader gains. |
| heading set | What, Why, When to use, Example, What proves it. | Overview, Background, How it works, Design decisions, Tradeoffs, Example, What proves it, Related. |
| diagram | one `flowchart LR`, exact source given. | same `flowchart LR`, preceded by a sentence naming its two inputs. |
| citations | inline `path:line` in prose plus a plan-vs-code table. | inline `path:line` in prose, a plan-vs-code table, and a `v8/fixtures/...` label above each `dl7` block. |
| sentence length | long compound sentences, several clauses each. | short declarative sentences, one claim per sentence. |
| tables | two: plan-vs-code, what-proves-it. | two: plan-vs-code (kept, moved under `## Design decisions`), what-proves-it. |
| length in lines | 154 | 200 |

## Dropped or changed facts

- `Chris, 2026-09-14:` attribution and "Amendment 3, same day". Changed: the design content and its `plans/v8/2026-09-14-v8-effect-demand.brief.md` citation survive in Background and Example; the speaker's name is author narration, which the skill's compactness rule removes. The date survives in the cited filename.
- `; fixture: v8/fixtures/host_effect/1_settled.dl7` and `; fixture: v8/fixtures/host_effect/3_loading.dl7:21-26` comment lines. Dropped: they are not fixture bytes; the required fixture path now stands as a line above each block.
- Input `1_settled` block moved the fixture's first comment below a `; fixture:` line. Changed: the block is now the file's exact byte order.
- Input `3_loading` block showed only `3_loading.dl7:21-26`. Changed: the block is the whole file, because the brief requires a `dl7` block to be a byte-for-byte fixture copy; the loading rule and its comment are unchanged.
- `## What`, `## Why`, `## When to use` headings. Changed to `## Overview`, `## Background`, `## Tradeoffs` to match the Explanation template and the skill's predictable-order rule. Every fact under them survives.
- No fact was dropped without a replacement. No unresolved factual gap remains.

## Skill questions answered on the user's behalf

none. The skill names no question to ask; its only open choice is the document type, resolved to Explanation from `references/document-types.md` because the page explains why and how something works.

## Checkers run

The skill ships no checker for an output page. Its scripts validate the skill repository itself.

`mdbook build` (from `v8/book`, required by the brief):

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../v8/book/src/modules/../probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../v8/book/book`
EXIT=0
```

Skill checkers (from the skill repo), neither reads the output page:

```
$ python3 scripts/validate_repo.py
Repository validation passed.
EXIT=0

$ python3 -m unittest discover -s tests
........
----------------------------------------------------------------------
Ran 8 tests in 0.005s

OK
```

Fixture byte-equality check over the output's `dl7` blocks:

```
len 360 -> ['1_settled.dl7']
len 531 -> ['3_loading.dl7']
len 141 -> ['2_source.dl7']
```
