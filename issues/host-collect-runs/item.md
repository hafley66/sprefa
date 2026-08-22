---
created: 2026-08-21
updated: 2026-08-21
type: bug
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# host_collect runs demand rows sequentially: 8 conditional GETs = 26s first poll, over the 10s law

## Description


## Finding (ghcache-compiles lane live run, 2026-08-21)

`v_tick_cost` bucket 29789428, the first poll of 8 endpoints (4 repos x events + branches):
`wall_ms = 26097`, all of it in `host_collect`. Bucket 29789427 (repeat 304 polls) was 122ms.
`HostLiveRunner::collect` (`hosts.rs:1620`) walks demand rows and calls `executor.run` one
at a time; `HttpFetchExecutor` blocks on ureq per row. Network-bound executors must answer a
demand batch concurrently: one bounded pool (the `apply_daemon_budget` cap) per tick, rows
dispatched together, results joined in demand order so the fold is deterministic. COUNT and
wall receipts: 8 endpoints at a 3s stub server = under 4s, not 24s. Scope: `hosts.rs collect`,
`executors/fetch.rs`, `executors/graphql.rs`; in-process executors (extract, soopy) stay
sequential unless measured otherwise.
