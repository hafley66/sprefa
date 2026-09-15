# xamfonos x f41

Skill `vendor-technical-writing-best-practices` (entry `SKILL.md`, 26 principles in `technical-writing-style-guide.md`), model `f41` (`openrouter/deepseek/deepseek-v4.1-flash`).

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Each sentence should have a single purpose." | editor compression throughout; openers are one clause each |
| "Write section headings as outcomes" | `## Serving a relation`, `## Which goals write no row`, `## Reading effect rows in a rule` |
| "Open each section with the main takeaway (BLUF)." | first line of every section states the outcome before the citation |
| "Before any technical concept, establish the problem it solves." | the opening frames outside-owned rows before `effect` is named |
| "Support every claim with code, data, or output that proves it." | every behavioral sentence carries a `src/...` range or fixture path |
| "Replace broad claims with concrete outcomes." | no promotional adjectives; outcomes are row shapes and exit codes |
| "Explain the reason for every step." | the settled-row example says why both goals produce a row |
| "Prove a concept through one concrete example rather than listing abstract claims." | `1_settled.dl7` proves the hit and miss case together |
| "Use real filenames, paths, and ports." | `fixtures/host_effect/1_settled.dl7`, `bash book/show.sh eval ...` |
| "Use symmetrical bullets or tables to maintain visual and logical balance." | the no-row table and the paired use / do-not-use lists |
| "Remove transitions and qualifiers that don't add information." | no "it's important to note", no "furthermore", no preamble |
| "Introduce the idea first, then its nuances." | one-sentence row definition, then application and snapshot detail |

## Prev vs next

| aspect | prev (input page) | next (your page) |
|---|---|---|
| opening | `## What` then a mermaid block with no prose | two-sentence thesis stating the row and its purpose, then the diagram |
| heading set | What / Why / When to use / Example / What proves it | 12 outcome headings (Serving, What the row holds, Which goals write no row, Reading, When a served relation earns an effect row, three Examples, Where plan and code disagree, Design history, What proves it) |
| diagram | one flowchart; `positive -->|heads a rule| noeffect` makes `noeffect` a leaf | same topology; `none` is one declared node fed by the rule, negative and aggregate cases |
| citations | inline `path:range` shorthand, mixed `src/...` and bare file | named `src/...:range` per claim; plan-vs-code table keeps the plan column |
| sentence length | several sentences with 3+ clauses and multiple citations | one idea per sentence, citations split across sentences |
| tables | 2 (plan-vs-code, what proves it) | 3 (which goals write no row, plan-vs-code, what proves it) |
| length in lines | 154 | 199 |

## Dropped or changed facts

- The `; fixture: <path>` marker line inside each ```dl7 block is removed and the path is stated above the block. Reason: the lane law requires a byte-for-byte copy of the fixture file, and that marker line is not in the file.
- The loading block shows all of `fixtures/host_effect/3_loading.dl7` (26 lines) instead of the input's `:21-26` range. Reason: byte-for-byte copy of a file, not a range.
- No fact is dropped. Every claim in the input page survives: the `--serve` text, the served-set-empty case, the every-evaluation rule, the `intern`/`none` application, the `intern_snapshot` copy, the negative/aggregate exclusions, the kernel arity and keys, the current-stratum read, the loading rule, the source case, the unknown-name diagnostic, the settled/source/loading examples, the plan-vs-code disagreements, the design history and the proof table.

## Skill questions answered on the user's behalf

none. The skill poses only self-check prompts ("Ask: Have I earned the right to introduce this idea?"); it puts no question to the user.

## Checkers run

the skill ships none.

Byte-for-byte fixture gate (extracted every ```dl7 block and compared to its fixture):

```
$ python3 - <<'EOF' ... compare blocks to fixtures ...
block match: ['1_settled'] len 360
block match: ['3_loading'] len 531
block match: ['2_source'] len 141
```

Book build (brief Step 5):

```
$ cd v8/book && mdbook build 2>&1 | tail -5
 WARN Caused By: failed to read `.../book/src/probes/21_partial_in_field.dl7`
 WARN Caused By: No such file or directory (os error 2)
Warning: The mdbook-mermaid preprocessor was built against version 0.5.0 of mdbook, but we're being called from version 0.5.4
 INFO Running the html backend
 INFO HTML book written to `.../book/book`
```

The three `ERROR Error updating "{{#include ../probes/{20,21,22}_*.dl7}}"` lines are pre-existing at base commit `824831a8d`: `book/src/probes/` lacks those three files and the output page does not include them. This page is not listed in `SUMMARY.md`, so it adds no build error.

Book block checker (scans only `book/src/*.md`, so it does not cover this file; run to show no regression on the tracked chapter):

```
$ bash book/check_blocks.sh
blocks 46 diff-clean
```

That checker requires the `; fixture:` marker as the first block line, which conflicts with the lane's byte-for-byte copy law; the byte-comparison above is the substitute for this file.
