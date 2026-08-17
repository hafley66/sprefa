---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: done
priority: high
epic: soopy-full-wiring
related: ['@extract-rev-pin-identity']
closed: 2026-08-16
---

# Extract host reads bytes through soopy read_each

## Description

SprefaExtractExecutor reads worktree disk (hosts.rs:852) while the plan carries a rev-pinned digest; digest gates identity, never content. Replace with soopy ReadRequest{source, expected: Some(ContentId::GitBlob(digest))} + SourceTree::read_each, then sprefa_extract::dispatch(path, bytes, mask) -- the working shape at source_bind/_1_runtime.rs:220-232. Closes the mechanism behind @extract-rev-pin-identity; gate: just scip-combo flips green for the right reason. Ref: entanglement doc candidate 1.

## Comments

### 2026-08-17T02:58:07Z · @soopy-driver

VERIFIED LANDED at origin/main a4045153e (PR #310). hosts.rs:115-119 holds a long-lived GitBatch per repo root (Mutex<BTreeMap>); read_blob at :817-833 uses soopy::GitBatch::open + .read(ObjectId). Digest branch at :886-899, the no-digest std::fs::read fallback at :903 is the only raw read left and is commented as such.
