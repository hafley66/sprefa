---
created: 2026-08-16
updated: 2026-08-16
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
