---
created: 2026-08-16
updated: 2026-08-16
type: chore
status: open
priority: low
---

# A14: comment_span excludes trailing comments (default picked)

## Description

## Decisions

### 2026-08-17T00:44:14Z · @chris

Stop 7 DECIDED 2026-08-16 (coordinator preference, user delegated): a comment_span never merges a trailing comment on a code line; a trailing comment is its own span. Matches the docs-from-comments rail, which reads leading blocks only (plans/2026-08-10-docs-from-comments.PLAN.visual.human.unga.md:86-92 left A14 open and unforced). Card stays open only to verify extraction actually behaves this way when comment work next wakes; if it already does, doc-close.
