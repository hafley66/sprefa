---
created: 2026-08-16
updated: 2026-08-17
type: chore
reporter: chris
status: done
priority: normal
epic: soopy-full-wiring
closed: 2026-08-16
---

# 0_query.rs cat-file spawn becomes soopy GitBatch

## Description

0_query.rs:60-90 hand-rolls one git cat-file spawn per blob; soopy::GitBatch::open + read (the batched form at change_facts.rs:193-205) is one long-lived process. Candidate 5.

## Comments

### 2026-08-17T02:58:07Z · @soopy-driver

VERIFIED LANDED at origin/main a4045153e (commit a16a16a83). 0_query.rs cat_blob:60-69 is soopy::GitBatch::open + .read(ObjectId); no hand-rolled Command spawn remains. Residual nit, not reopened: cat_blob calls soopy::discover(".") rather than discover(path), so a --digest read resolves the repo from cwd.

### 2026-08-17T04:13:59Z · @soopy-driver

PR #337 merged at 55e15e7478918a2a7c8b0c63ad4679f00250d099: `extract query --digest` discovered the repository from the CWD instead of from the queried PATH (0_query.rs `cat_blob` called `soopy::discover(".")`). Fix: `cat_blob(path, oid)` discovers from `path.parent()`. Graded by soopy-driver in the lane worktree, `cargo test --features cli` 143/0 twice, then 143/0 again on merged main.

Two notes on the lane's own header. Its SABOTAGE 2 claim ("discover from `path` rather than `path.parent()`: a file path that is not a directory fails to discover") does not hold: `soopy::discover` already falls back to the parent for a non-directory (`_2_repository.rs:10-15`), so the `.parent()` call is belt-and-braces rather than the operative part. And the empty-parent case (a bare relative filename, whose `Path::parent()` is `Some("")`) degrades to `git -C ""`, which git treats as the cwd, so the pre-fix behavior survives for that spelling; measured, rc=0.

