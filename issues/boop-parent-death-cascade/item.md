---
created: 2026-08-16
updated: 2026-08-19
type: feature
status: done
priority: normal
labels:
- area:boop
closed: 2026-08-19
---

# boop: parent death cascades to children

## Description

User ask 2026-08-17: "does boop make it possible for the parent dying to cause
children to also close?" Answer today: no. The parent edge is consumed only by
the on-exit completion hail (crates/boop/src/supervise.rs:267-288) and pstree
rendering, where an orphan hangs under a `[gone]` root (main.rs:3211-3228).
No code watches parent liveness.

Incident that motivates it: 2026-08-17 the sprefa coordinator process
restarted; two native opus drivers and their opus rigs died silently, three
flash4 lanes were already dead. Nothing told the survivors, nothing reaped.

## Acceptance Criteria

- [x] Lane create / agent register accept a policy: on parent death, kill me,
      or keep me and re-parent to the coordinator, or keep me orphaned (today).
- [x] Supervisor detects parent death (registry route gone + pane/pid gone) and
      applies the policy within one poll interval.
- [x] Orphans that survive show a typed reason in lane list; pstree keeps the
      `[gone]` root.
- [x] Test: spawn parent + child, kill parent, assert child outcome per policy.

## Landed

`--on-parent-death kill|reparent|orphan` on `beep lane create` and
`beep agent register` (`crates/boop/src/supervise.rs` `ParentDeathPolicy`,
`record_parent_policy`); the supervisor checks parent liveness on its existing
poll interval and applies the policy within one interval
(`ParentWatch::probe`, `parent_alive`). A dead lane's reason and a surviving
orphan's row both name the edge: `DEAD=parent-died=<parent>`,
`DEAD=reparented=<parent>`, `PARENT-GONE=<parent>` (`crates/boop/src/trail.rs`
`DeadReason`, `main.rs` `run_lane_list`, `gone_parent`). Per-policy tests in
`crates/boop/tests/parent_death.rs`.

Gap: a pane-less native agent (`beep agent register`) runs no supervisor of
its own, so its recorded policy is stored and nothing polls on its behalf;
only a lane with a supervisor process enforces kill or reparent.
