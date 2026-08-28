---
created: 2026-08-24
updated: 2026-08-24
type: bug
reporter: sprefa-coordinator
status: open
priority: normal
labels: [ci, prolog]
---

# prolog-lint on main reads 18 findings, CI-KNOWN-RED allows 14

## Description

Measured by lane plan-prolog-split on base sha 9e4b468157bb2a189960b8ec69daad10af372862: prolog-lint findings=18, baseline=0; .github/CI-KNOWN-RED.md:117 records 14. Four findings landed on main since the allowlist row was written. Per CLAUDE.md, a failing leg not matching the allowlist is the real signal. Next: run the lint on main, list the 4 new findings with file:line, fix or re-decide each, then correct the allowlist row. Note from the same lane: the 12 existing 0_generic_expand/ include parts draw zero findings, so include-shaped splits are clean under this gate.
