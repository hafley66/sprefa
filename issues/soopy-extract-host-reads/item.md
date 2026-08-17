---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: open
priority: high
epic: soopy-full-wiring
related: ['@extract-rev-pin-identity']
---

# Extract host reads bytes through soopy read_each

## Description

SprefaExtractExecutor reads worktree disk (hosts.rs:852) while the plan carries a rev-pinned digest; digest gates identity, never content. Replace with soopy ReadRequest{source, expected: Some(ContentId::GitBlob(digest))} + SourceTree::read_each, then sprefa_extract::dispatch(path, bytes, mask) -- the working shape at source_bind/_1_runtime.rs:220-232. Closes the mechanism behind @extract-rev-pin-identity; gate: just scip-combo flips green for the right reason. Ref: entanglement doc candidate 1.
