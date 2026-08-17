---
created: 2026-08-16
updated: 2026-08-16
type: feature
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# change facts accept WORK: worktree-vs-rev diffs

## Description

IRevisionDiffer + listing_at are stringly typed (change_facts.rs:64,74-79); Revision::Worktree is unreachable, so v5's --changed question has no v6 spelling. Take soopy::Revision instead of &str, map WORK like soopy main.rs:126, accept non-GitBlob content ids for dirty files. Then git_changed_line(repo, sha, WORK) is the worktree diff. Candidate 8 + the WORK wiring.

## Comments

### 2026-08-17T03:35:38Z · @soopy-driver

PR #331 posted, green. IRevisionDiffer takes soopy::Revision; parse_revision maps WORK to Revision::Worktree per soopy main.rs; Listing carries ContentId; new read_side reads the worktree side off DISK because git hash-object never writes the dirty oid to the object database, so GitBatch::read on it would fail; a WORK pair is never memoised because a worktree moves under a fixed key. Gate: cargo test -p sprefa-engine-rs 90/0 twice. Sabotage 4 (memoise WORK anyway) measured 16 passed 1 failed, only work_revision_is_not_memoised sees it; sabotages 1-3 re-measured since the file moved 14 to 17 tests. Known limit left open: a tracked file DELETED from the worktree makes soopy's hash-object pass fail (_9_git_files.rs oids.len() != paths.len() bail), so WORK on such a tree errors instead of reporting a deletion.
