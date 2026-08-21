---
created: 2026-08-21
updated: 2026-08-21
type: improvement
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
labels: [engine, prolog, perf]
---

# Host response storage: one table per extraction, set-shaped dedupe, no constant columns

## Description

Four host declarations over one extract run store four response tables; UNIQUE(witness_digest, ordinal) keeps 18708 occurrence rows where 5310 distinct (path, callee) exist; record/family are filtered to literals in every rule yet stored per row. Lowering change in lower.pl host expansion plus hosts.rs. After Lane A.
