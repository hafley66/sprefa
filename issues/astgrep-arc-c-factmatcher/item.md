---
created: 2026-08-25
updated: 2026-08-25
type: task
reporter: hafley66
status: done
priority: normal
epic: extract-astgrep-soopy
labels:
- pkg:extract
related: ['@astgrep-arc-a-languages', '@astgrep-arc-b-drain']
closed: 2026-08-25
commits:
- hash: PR#475
  summary: merged
---

# Arc C: FactMatcher over dl6.db and extract move as one YAML rule

## Description

Plan section: PLAN.md '## Arc C'. FactMatcher { rel, column, value } implements ast_grep_core::Matcher reading ~/.agent/dl6.db read-only, composed with ops::All/Any/Not; extract move re-expressed as one YAML rule plus a fact matcher, byte-identical on tests/1_move.rs fixtures and on the real tree. Blocked by Arc A and Arc B.
