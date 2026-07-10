---
name: feedback-descriptive-dl-var-names
description: "HARD RULE: no single-letter/cryptic variable names in dl snippets (skills, examples, docs, tests, agent prompts); use path/line/prompt_text, never p/l/q"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 4e6d11b9-5d44-4352-bd43-76b4f9c29ee7
---

Chris (2026-07-04): "for the love of fucking christ can we stop proliferating
one word fucking var names, this is out of hand, i cant read any of this
because its all ps and qs."

**Why:** dl rules are joins; the variable NAME is the only signal of what a
column means at a glance. `hit(p, l, cap)` reads as noise; `hit(path, line,
effect_body)` reads as a sentence.

**How to apply:** every dl snippet written or reviewed (skill quickstarts,
examples/*.dl, book chapters, test fixtures, agent-prompt specs) uses
descriptive names: `path` not `p`, `line` not `l`, `session`/`seq`/`title` not
`s`/`t`/`x`. Rule capture vars name the thing captured (`use_path`,
`callee_name`). Agent prompts for dl work must carry this rule explicitly.
Existing files get renamed opportunistically when touched. Related:
[[feedback-no-casual-codenames]].
