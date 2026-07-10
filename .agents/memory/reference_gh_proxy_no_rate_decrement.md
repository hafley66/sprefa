---
name: reference_gh_proxy_no_rate_decrement
description: "in this env `gh api` does NOT decrement core rate limit (used=0 / rest_remaining=null) — use request COUNT + 304 ratio as the currency for live-GitHub tests, not a rate-limit delta"
metadata: 
  node_type: memory
  type: reference
  originSessionId: bcd97199-357a-4558-99c0-8c9d162efb3f
---

In this environment `gh api` calls route through a proxy that does **not**
decrement the GitHub core rate limit. Verified 2026-06-29: `gh api rate_limit`
shows `used=0 remaining=5000` even after fresh distinct fetches
(torvalds/linux, rust-lang/rust, golang/go, nodejs/node) and after the dl
ghcacher port fired 402 live `gh api` requests over the kubernetes org.
ghcacher saw the same thing (`rest_remaining: null` in its call log).

Consequence for any live-GitHub test (gh-cache.dl, the ghcacher head-to-head):
a rate-limit before/after delta is NOT an observable signal here. Use the
**request count** and the **conditional-cache 304 ratio** as the "polite to
GitHub" currency instead, read from the tool's own logs (dl: `pending_effect`
count + `rel_resp` status breakdown; ghcacher: `call_log`).

Both sit behind the [[project_sh_effect_runtime]] cache loop; the repeatable
comparison is `v5/bench/ghcacher_vs_dl.sh`. Auth: `gh api user` = hafley66.
