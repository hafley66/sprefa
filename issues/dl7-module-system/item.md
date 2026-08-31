---
created: 2026-08-31
updated: 2026-08-31
type: epic
owner: chris
status: open
priority: high
labels: [compiler]
---

# DL7 module system parity

## Description

## Description

Evolve V7 from the current combined prelude-plus-file unit into separately owned modules. Reuse the applicable implementation and fixtures from v6/prolog/use_resolve.pl, v6/prolog/0_dot_expand.pl, v6/prolog/executor_modules.pl, and their module-path tests.

## Acceptance Criteria

- [ ] Prefix import, alias, and export syntax is decided.
- [x] Separate files preserve declaring-module type identities.
- [ ] Dotted and aliased references resolve through one path model.
- [ ] Cycles and ambiguous names produce positioned diagnostics.

## Tests Run

- [x] Consolidated module fixture passes.
- [x] Existing V7 gates pass.

## Agent Runs

### 2026-08-31T06:20:41Z · @codex

Internal module substrate reached milestones 1-9 from plans/2026-08-31-dl7-module-system.md. Milestone 10 remains the user-selected prefix import/export syntax and its parser integration.
