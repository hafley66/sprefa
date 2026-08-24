---
created: 2026-08-23
updated: 2026-08-23
type: bug
status: open
priority: high
---

# One-db storage grows unbounded: telemetry rows, HTTP body cache, string dictionary

## Description

## Comments

### 2026-08-24T02:15:17Z · @sprefa-coordinator

Measured 2026-08-23 on ~/.agent/dl6.db (90 MB, 157 tables, freelist 0): engine_tick_cost 110,838 rows AND its twin __host_response_dl__tick_cost 110,838 rows (ghcache.dl6:1361 arrives every bucket, nothing retracts; buckets span ~20 h), __host_response_http__get 40 MB / 12,707 rows (cached bodies, no eviction), __str 33,584 rows / 10 MB + 12 MB autoindex (interning is permanent by design). Fixes are language-adjacent (retention/ttl policy per rel, cache eviction, dictionary GC) so design needs Chris; no lane until the fork is decided.
