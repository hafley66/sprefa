---
created: 2026-08-23
updated: 2026-08-23
type: improvement
reporter: hafley66
status: open
priority: high
related: ['@ordered-tick-recompute']
labels: [engine]
---

# One transaction per tick: a SIGKILL mid-tick leaves a level stale until a read rel moves

_Source: v6/sprefa-engine-rs/src/sql.rs_

## Description

Since #423 a level recomputes only when a rel it reads moved. The tick is not one SQLite transaction, so a process killed between a write and the level recompute leaves that level stale; the first tick after open rebuilds every level (the TEMP-table first_fold detector in ordered.rs), which heals it at boot, and a kill that the same process survives does not exist, so the window is real only across restarts that happen to skip the first-tick rebuild (none today). The proper fix: BEGIN at tick start, COMMIT after promote, in sql.rs around the seam, so a killed tick leaves the db at the previous tick. Receipt: a test that kills (or aborts) mid-tick and reads the db back at the previous tick's state; statement count unchanged.
