---
created: 2026-08-14
updated: 2026-08-16
type: task
status: open
priority: high
epic: v5-behavioral-parity
labels:
- parity
- v6
- size:med
lane: engine-source-bind
lane_seq: 20
collision: [source-bind-runtime, store-schema]
---

# Restart-safe deletion projection for SourceBind

## Description

## Goal
Restart-safe deletion projection. The identity store is durable, but SourceBind keeps authored file/span/extraction rows in in-memory maps needed to construct later retractions. Make deletion projection survive a restart.
## Where to put it
- v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs — the in-memory map that holds rows for later retraction.
- Identity store durability lives in v6/sprefa-store; reconcile the in-memory maps against it so a restart rebuilds the retraction set.
## Perf gate
- v6/sprefa-engine-rs: just dd-grade / just rust-grade (retract arm graded against the oracle tick log)
- v6/justfile: just memory-soak (assert/retract churn; memory, sqlite page count, statements/tick stay flat)
## Implementation Notes
On restart the retraction projection must be reconstructible from the durable identity store alone, not from the lost in-memory maps.
