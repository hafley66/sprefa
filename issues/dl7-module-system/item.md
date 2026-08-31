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
- [ ] Separate files preserve declaring-module type identities.
- [ ] Dotted and aliased references resolve through one path model.
- [ ] Cycles and ambiguous names produce positioned diagnostics.

## Tests Run

- [ ] Consolidated module fixture passes.
- [ ] Existing V7 gates pass.
