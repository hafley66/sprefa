---
created: 2026-08-17
updated: 2026-08-17
type: chore
reporter: sprefa-coordinator
status: open
priority: normal
labels:
- area:boop
---

# hafley-rs CI: cargo test without --no-fail-fast hides every integration target behind two host-dependent lib failures

## Description

## Comments

### 2026-08-17T13:15:19Z · @sprefa-coordinator

Measured by boop-highs-driver 2026-08-17: the runner lacks codex and just, so two lib tests fail and the run stops before any integration target; PR #14's 8 new tests were graded by nobody in CI. Fix: --no-fail-fast, and mark the two host-dependent tests #[ignore] unless the tool is on PATH (or gate them behind a cfg/env). Separate: cargo-dist plan job red since the soopy path dep landed (Chris's call: git/registry dep).
