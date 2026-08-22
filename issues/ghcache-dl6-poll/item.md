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

# ghcache.dl6: poll periods compared as raw seconds against minute-quantized clock buckets (60s config = 60 minutes)

## Description


## Finding (ghcache-compiles lane, 2026-08-21)

`period_candidate/2` (`v6/dl/ghcache/ghcache.dl6:266-283`) feeds `global_setting.poll_period`
(raw seconds, 60) straight into `endpoint_period` as a mod divisor against
`current_clock(60, Bucket)`, whose Bucket increments once per real MINUTE
(`clock.rs bucket_of = now_secs / every`). Never divided by the clock granularity, so
`due` fires every 60 buckets = 60 minutes for `poll_interval_seconds = 60`. Same for
`org_discovery_period`, the `X-Poll-Interval` candidate, and the warn-stretch candidate.
Masked because every simulated schedule fed Bucket as raw seconds (0, 60, 120).

Fix shape: one `rel clock_granularity(secs: int)` seed = 60; every period candidate is
`ceil(secs / granularity)` buckets; the simulated schedules feed Bucket as minute units;
a COUNT test: `poll_interval_seconds = 60` over 3 real buckets = 3 polls.

Live smoke for #ghcache-compiles used `poll_interval_seconds = 1` as a workaround; restore
`60` in `~/.config/ghcache/config.toml` when this lands.
