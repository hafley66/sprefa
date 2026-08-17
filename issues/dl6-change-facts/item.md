---
created: 2026-08-14
updated: 2026-08-16
type: feature
status: done
priority: high
epic: v5-behavioral-parity
labels: [parity, v6]
closed: 2026-08-16
closed_by: fable
---

# DL6 relations for change facts (changed / changed_line / created / deleted / modified / renamed)

## Description

## Goal
Author DL6 relations for changed, changed_line, created, deleted, modified, renamed (the V5 change-fact family).
## Where to put it
- v6/sprefa-engine-rs/src/source_bind/_0_types.rs — add columns to SourceBindRelations declarations.
- v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs — arrivals for the change relations.
- Authored source.dl6 schema; feed from git diff between revs via the SoopyFilesExecutor path in hosts.rs.
## Perf gate
- v6/justfile: just precommit-changed (V5 git-fact diags rail on a real four-commit repo; row-set equality, not counts)
## Implementation Notes
changed_line joins a rev-pinned files_at against a base with not/1. Every assertion is sorted row-set equality so a rail firing on every new file fails on the control.

## Resolution

### 2026-08-16T05:48:39Z · @fable

Landed PR #293: git_change/git_rename/git_changed_line on the demand path, imara-diff (measured build-vs-buy), 11_change_gate 5/5 byte-identical x3, precommit-changed rail holds x3.
