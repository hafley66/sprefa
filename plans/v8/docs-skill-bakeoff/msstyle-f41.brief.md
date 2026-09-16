# Lane brief: rewrite v8/book/src/10_effects.md under the "msstyle" documentation skill, model f41

## First action

```bash
git merge --ff-only 824831a8d594d3e985a3d268eeaca76a1a09ef92
```

If that fails, stop and report with the error text. Do not work around it.

## The experiment

Read `/Users/chrishafley/projects/sprefa/plans/v8/docs-skill-bakeoff/README.md` (absolute path; it is not on your branch). You are one cell of that grid: skill `msstyle`, model `f41`.

## Step 1. Load the skill, fully

Read every file under the skill directory. Entry file:

```
/Users/chrishafley/projects/claude-research/skills_archive/vendor-claude-ai-technical-writing-skill/SKILL.md
```

Use its `workflows/create.md` path. The project has no lint command; skip that phase and say so.
Read the entry file, then every file it links or names (references, workflows, style guides, scripts). The skill's rules govern the shape, order, headings, sentence style, and verification of your output. Where the skill says to ask the user a question, do not ask; pick the option the skill itself recommends and record the choice in NOTES.md.

## Step 2. Read the input page and the code it cites

Input (do not edit it): `v8/book/src/10_effects.md`.

Every `path:line` in that page points under `v8/src/` or `plans/v8/` (paths without a leading `v8/` are relative to `v8/`). Open each cited range. Keep a citation only if the lines say what the page says. Fix the range if it moved. Drop the sentence if the code does not say it, and record the drop in NOTES.md.

Fixtures live under `v8/fixtures/`. A ```dl7 block in your output must be a byte-for-byte copy of a file there; say which file above the block. Never write a dl7 program from your head.

## Step 3. Write the rewrite

Output file: `v8/book/src/lab/docs-skill-bakeoff/msstyle/f41/10_effects.md`

Hard laws that hold regardless of the skill:

- Every fact in the input page survives into the output, or NOTES.md names it as dropped and why.
- No new claim about how dl8 behaves unless a `v8/src` line or a `v8/fixtures` file shows it, cited beside the claim.
- The page is for a reader learning the language, not for the author. No "we", no "let's", no narration of your process.
- Mermaid renders in this book (`mdbook-mermaid`). Use it or not as the skill directs.

## Step 4. Write NOTES.md

File: `v8/book/src/lab/docs-skill-bakeoff/msstyle/f41/NOTES.md`

Sections, in this order:

1. `# msstyle x f41` and one line: which skill file and which model.
2. `## Rules applied`: a table, one row per skill rule you followed, columns `rule (quoted from the skill) | where it shows in the output`.
3. `## Prev vs next`: a table, columns `aspect | prev (input page) | next (your page)`. Rows at least: opening, heading set, diagram, citations, sentence length, tables, length in lines.
4. `## Dropped or changed facts`: list, each with the reason.
5. `## Skill questions answered on the user's behalf`: list, or "none".
6. `## Checkers run`: the exact commands and their output, or "the skill ships none".

## Step 5. Verify

```bash
cd v8/book && mdbook build 2>&1 | tail -5
```

Must print no error. Run any checker script the skill ships against your output file and paste the output into NOTES.md.

## Ownership

You own only `v8/book/src/lab/docs-skill-bakeoff/msstyle/f41/`. Forbidden: `v8/book/src/10_effects.md`, `v8/book/src/SUMMARY.md`, anything under `v8/src`, `v8/fixtures`, `plans/`. Never spawn subagents.

## Commit

One commit, subject exactly:

```
lab(docs-skill): msstyle x f41 rewrite of 10_effects
```

Do not push. Do not open a PR.

## Done

```bash
boop beep --no-wait --as lab-docs-skill-msstyle-f41 sprefa-coordinator "done: msstyle x f41, commit $(git rev-parse --short HEAD), output lines $(wc -l < v8/book/src/lab/docs-skill-bakeoff/msstyle/f41/10_effects.md)"
```
