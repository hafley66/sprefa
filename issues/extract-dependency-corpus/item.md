---
created: 2026-09-08
updated: 2026-09-08
type: feature
reporter: fable
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# extract: index node_modules/<pkg> on demand and follow resolved imports into it

## Description

## Description

The eyes lane won 24 of its 34 rows by reading `node_modules/@vitest/runner` and `playwright-core` dist files. extract can index them (TypeScript/JavaScript) but nothing points it there. Add `--corpus <dir>` (repeatable) that runs the scip-typescript path over a dependency and lets `resolved_import` follow into it, so a reviewer can ask what signature `waitForURL` takes without leaving the fact stream. See research/2026-09-08-fact-sources-beyond-scip.md §1 gap 4, §5 row 3.

## Acceptance Criteria

- [ ] `extract --family scip --corpus node_modules/playwright-core .` emits scip rows for the dependency with a `corpus` field
- [ ] `resolved_import` from package source resolves into the corpus symbol
- [ ] cache keyed by package name + version

## Tests Run

- [ ] fixture with one dependency

## Implementation Notes

- [ ] reuse scip_ensure; the corpus root is the package dir with its own tsconfig or a synthesized one
