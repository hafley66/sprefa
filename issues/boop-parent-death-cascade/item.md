---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
labels:
- area:boop
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

- [ ] Lane create / agent register accept a policy: on parent death, kill me,
      or keep me and re-parent to the coordinator, or keep me orphaned (today).
- [ ] Supervisor detects parent death (registry route gone + pane/pid gone) and
      applies the policy within one poll interval.
- [ ] Orphans that survive show a typed reason in lane list; pstree keeps the
      `[gone]` root.
- [ ] Test: spawn parent + child, kill parent, assert child outcome per policy.
