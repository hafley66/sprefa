---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-module-system
labels:
- size:med
size: M
lane: dl7-module-system
lane_seq: 0
collision: [v7-reader, v7-compiler, v7-test]
blocked_by: ['@dl7-type-algebra-oracle']
commits:
- hash: 8aa7aa9e1
  summary: Compile DL7 filesystem modules through colon edges
closed: 2026-08-31
closed_by: codex
---

# Compile DL7 filesystem modules through colon edges

## Description

Port the reusable file ownership and basement merge mechanics into V7, then
represent module containment with the same node and edge model used by types.

## Acceptance Criteria

- [x] V7 files compile as separate module-owned units.
- [x] Canonical source paths provide stable file-module identities.
- [x] Prelude constructors remain available without copying user modules.
- [x] Filesystem products and containment edges join the merged basement.
- [x] `:/4` works in rule heads and bodies.
- [x] Bare argument-2 labels in `:/4` lower as constants.
- [x] A sibling file's type identity is reached through an ordinary colon goal.
- [x] Host path-list and import-row resolution code is removed.

## Tests Run

- [x] Nested filesystem graph fixture passes.
- [x] Cross-module compiler fixture passes.
- [x] V7 SWI suite passes 32/32.
- [x] V7 Tree-sitter corpus passes 1/1.

## Agent Runs

### 2026-08-31 · @codex

Implemented in commits `349de4645`, `4148743e0`, and `8c2f3d65e`, merged by
PR 618 as `8aa7aa9e1`.

## Resolution

### 2026-08-31T22:19:26Z · @codex

Filesystem products and colon traversal merged in PR 618. Deferred edge syntax remains outside this task.
