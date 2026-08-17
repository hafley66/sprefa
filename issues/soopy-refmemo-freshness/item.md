---
created: 2026-08-16
updated: 2026-08-16
type: bug
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# GitRefExecutor memo never invalidates: keyed on repo alone

## Description

hosts.rs:440 memoises ref snapshots per repo with no rev, name, or mtime in the key; refs move, the memo does not. Add a freshness witness to the key (or soopy RepositoryWatcher, currently dead surface). Candidate 10.
