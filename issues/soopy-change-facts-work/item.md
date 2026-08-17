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
