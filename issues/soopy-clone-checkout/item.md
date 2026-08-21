---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# soopy: clone and checkout exports for the engine's checkout executor

## Description

soopy exports no clone and no checkout: Acquisition::execute covers FetchRef, FetchTag, Deepen, Unshallow (_13_fetch.rs:66-71); clone/checkout exist only private in _14_multi_repo_refresh.rs:305-321. Wanted: clone(url, dest) -> Repository and checkout(&Repository, ObjectId). Until then soopy_checkout observes an existing checkout and names soopy_clone_missing. hafley-rs work.
