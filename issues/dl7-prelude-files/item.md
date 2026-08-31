---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-type-algebra
labels:
- size:med
size: M
lane: dl7-type-algebra
lane_seq: 0
collision: [v7-prelude, v7-compiler]
---

# Split the ordered DL7 prelude

## Description

## Description

Load all numbered prelude files in lexical order and split the 375-line monolith by dependency layer.

## Acceptance Criteria

- [ ] Existing compiler rows and behavior remain equivalent.
- [ ] Prelude files follow author-driven numeric reading order.
- [ ] Tree-sitter parses every prelude file.

## Tests Run

- [ ] Focused V7 compiler test passes.
