---
created: 2026-08-16
updated: 2026-08-16
type: bug
reporter: chris
status: open
priority: normal
---

# door skew family gate is load-sensitive: silent zero under CPU contention

## Description

Measured during soopy phase-1 grading (2026-08-16): clean main RED three times on 7_door_skew_family (rc=0, zero rows) while two lanes compiled; idle re-measure twice GREEN (named stop fires at rc=101). Under CPU contention the Rust door degrades from the named stop to the exact silent-zero-rows shape the pin exists to catch, so the leg emits false REDs in any parallel-lane round. Needs: reproduce under synthetic load, then either a contention guard in the gate or a root fix in the door timeout path.
