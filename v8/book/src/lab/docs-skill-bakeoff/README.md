# docs-skill bakeoff

One page, five third-party documentation skills, two models. Each lane rewrites the same page under one skill. Chris reads the results and cherry-picks by taste.

## Test page

`v8/book/src/10_effects.md` at `824831a8d` (origin/main). This is the PR #767 page Chris flagged: one paragraph, many inline citations, a diagram standing in for a list.

## Skills under test

| id | skill | entry file | origin |
|---|---|---|---|
| diataxis | Ferroman diataxis-documentation-skill | `~/projects/claude-research/skills_archive/vendor-diataxis-documentation-skill/skills/writing-documentation/SKILL.md` | github.com/Ferroman/diataxis-documentation-skill |
| toss | gwagjiug technical-writing (Toss guide) | `~/projects/claude-research/skills_archive/vendor-technical-writing/skills/technical-writing/SKILL.md` | github.com/gwagjiug/technical-writing |
| xamfonos | technical-writing-best-practices (26 principles) | `~/projects/claude-research/skills_archive/vendor-technical-writing-best-practices/SKILL.md` | github.com/Xamfonos/technical-writing-best-practices |
| msstyle | Claude-ai-technical-writing-skill (Microsoft Manual of Style) | `~/projects/claude-research/skills_archive/vendor-claude-ai-technical-writing-skill/SKILL.md` | github.com/pavithramailbox/Claude-ai-technical-writing-skill |
| ibook | ibook-skills chapter-content-generator | `~/projects/claude-research/skills_archive/vendor-ibook-skills/skills/chapter-content-generator/SKILL.md` | github.com/dmccreary/ibook-skills |

## Models

| id | preset | model |
|---|---|---|
| glm53f | `glm53f` | openrouter/z-ai/glm-5.3-flash |
| f41 | `f41` | openrouter/deepseek/deepseek-v4.1-flash |

## Output layout

```
v8/book/src/lab/docs-skill-bakeoff/<skill>/<model>/10_effects.md   the rewrite
v8/book/src/lab/docs-skill-bakeoff/<skill>/<model>/NOTES.md        skill rules applied, prev vs next table
```

Lane branch: `lab/docs-skill-<skill>-<model>`. Brief: `plans/v8/docs-skill-bakeoff/<skill>-<model>.brief.md`.

## What the lanes were and were not told

Told: apply the skill, keep every fact, verify every citation against `v8/src`, dl7 blocks are verbatim fixture copies, no invented semantics.

Not told: Chris's own docs law (diagram-beside-list, stage-setting opener, numbered files). The point is to see each skill's taste unmixed. Chris applies his laws after picking.
