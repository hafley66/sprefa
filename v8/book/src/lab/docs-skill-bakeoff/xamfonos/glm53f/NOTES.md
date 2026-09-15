# xamfonos x glm53f

Skill: `vendor-technical-writing-best-practices/SKILL.md` + `technical-writing-style-guide.md` (26 principles). Model: openrouter/z-ai/glm-5.3-flash.

## Rules applied

| rule (quoted from the skill) | where it shows in the output |
|---|---|
| "Open each major section with a single declarative idea" (Semantic Hierarchy / BLUF) | Opening sentence states the thesis: effect rows are the record of interest, the outside settles them, rules turn answers into data |
| "Write section headings as outcomes" (Declarative Storytelling) | "How a goal becomes an effect row", "Why there is no Host declaration", "What the rows look like" |
| "Every section must serve the core argument" (Thesis Spine / Thesis Integrity) | Every section extends the interest-record thesis; the plan-dispute table is kept because it shows the code's final semantics |
| "Support every claim with code, data, or output that proves it" (Code-Backed Authority) | Every behavioral claim carries a `path:line` citation; every example carries its console output |
| "Solutions introduced only after problems are fully established" (Earned Solutions) | The need for a record of outside requests precedes the mechanics; the amendment history precedes the worked examples |
| "Link ideas with clear reasoning... causal connectors" (Assertive Transitions) | "Because the row is interest rather than a miss report, a settled answer does not erase it"; "so both have an `effect` row" |
| "Break long sentences at each new action or dependency" (Reader Compression) | The former one-paragraph wall is split into short sentences, each with one mechanism |
| "Filler to cut" (Signal-to-Noise Density) | No "it is important to note", no "simply"/"just", no hedging ("seems", "might") |
| "Use the same verb form when describing alternatives" (Parallel Logic) | The use/do-not-use lists keep parallel clauses; the two-kinds-of-goal list is symmetric |
| "Annotate every non-obvious line" (Anti-Patterns) | Non-obvious fixture lines carry prose above each block; fixture comments are kept |
| "Prove a concept through one concrete example" (Show, Don't Stack) | Each semantic case (settled, loading, source, unserved, unknown name) has one fixture and one run |
| "Every new concept must be set up before it's explained" (Introduced Abstractions) | `intern_snapshot`/`cons` reading is introduced via the "live interest" amendment, then shown in the loading example |
| "Use real filenames, paths, and ports" (Fidelity of Examples) | Real fixture paths, real `book/show.sh` commands, real urls from the fixtures |
| "Check that every statement can be confirmed in code or configuration" (Technical Conviction) | Every `path:line` in the input was re-opened and verified; ranges corrected where they had drifted |
| "Keep explanations separate from instructions" (Semantic Hierarchy) | Concept sections (How/Why) precede the worked examples; the proof table is last |

## Prev vs next

| aspect | prev (input page) | next (this page) |
|---|---|---|
| opening | `## What` heading, then a mermaid flowchart before any prose | Declarative thesis sentence before any structure; the mechanism follows in prose |
| heading set | What / Why / When to use / Example / What proves it | How a goal becomes an effect row / Why there is no Host declaration / When to use it / What the rows look like / What proves it |
| diagram | mermaid flowchart of the effect branch | none; the flow is prose with one sentence per branch (skill has no diagram rule; prose chosen) |
| citations | inline, dense, several per paragraph | same citation set, verified and corrected, one beside each claim |
| sentence length | several multi-clause sentences in one paragraph | short sentences, one mechanism each, broken at dependency boundaries |
| tables | 1 (plan vs code) | 2 (plan vs code, what proves it) |
| length in lines | 154 | 195 |

## Dropped or changed facts

- The mermaid flowchart was removed. No fact was lost: every edge of the chart is a prose sentence in "How a goal becomes an effect row" with its citation. Reason: the skill's rules favor declarative prose with headings as outcomes and single-purpose sentences; the chart duplicated the prose and no skill rule directs diagrams.
- Citation ranges corrected, claims unchanged: `_6_json.rs:345-366` -> `:346-362` (`serve_relations` spans those lines); `_5_evaluate.rs:272-273` -> `:272-274` (the snapshot push sits in a three-line statement); added `_5_evaluate.rs:260-262` next to the doc comment that states "every evaluation ... hit or miss".
- The input's two partial dl7 snippets (the `1_settled` block with a fabricated header comment and the `3_loading` excerpt) were replaced by byte-for-byte fixture copies: `fixtures/host_effect/1_settled.dl7` in full, `fixtures/host_effect/2_source.dl7` in full, `fixtures/host_effect/3_loading.dl7:21-26` named above the block. The fabricated `; fixture: ...` comment lines are not in the fixtures, so they do not survive.
- No behavioral fact was dropped. All console outputs, the plan-dispute table, the use/do-not-use lists, the "What proves it" table, and the brief citations survive unchanged.

## Skill questions answered on the user's behalf

- none. The skill issues no user-facing questions; it directs "apply all five sections before writing".

## Checkers run

The skill ships no checker scripts (`ls` of the skill directory: `LICENSE`, `README.md`, `SKILL.md`, `technical-writing-style-guide.md`).

Verification performed:

1. Every `path:line` cited by the input page was opened against the code at this commit; all ranges either held or were corrected as listed above.
2. dl7 blocks diffed byte-for-byte against the fixtures:

```
blocks: 3
1_settled full copy: True
2_source full copy: True
3_loading 21-26 copy: True
```

3. Book build: `cd v8/book && mdbook build 2>&1 | tail -5` — no errors.
