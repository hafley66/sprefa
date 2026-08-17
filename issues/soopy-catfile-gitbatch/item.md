---
created: 2026-08-16
updated: 2026-08-16
type: chore
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# 0_query.rs cat-file spawn becomes soopy GitBatch

## Description

0_query.rs:60-90 hand-rolls one git cat-file spawn per blob; soopy::GitBatch::open + read (the batched form at change_facts.rs:193-205) is one long-lived process. Candidate 5.
