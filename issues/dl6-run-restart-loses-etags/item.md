---
created: 2026-08-22
updated: 2026-08-22
type: bug
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# `dl6 run` restart folds from scratch: every ETag is lost, every endpoint re-downloads at 200

## Finding (coordinator, live `dl6 run` of ghcache.dl6, 2026-08-22)

Three starts of `dl6 run v6/dl/ghcache/ghcache.dl6` in ten minutes. After each
start `ghcache_call_log` and `ghcache_rate_state` in `~/.agent/dl6.db` read 0
rows and the first poll bucket is 200 x 8 with ~1.0 MB of body
(`sum(bytes)` 955381, then 1034700). The 304/bytes=0 steady state only holds
within one process lifetime. The one-db decision (CLAUDE.md 2026-08-21) exists
so that state survives the process; the host response rows (`prev_etag`) are
the minimum that must persist across a restart. Backlog row
`host-response-storage` owns the fix; this is its receipt.
