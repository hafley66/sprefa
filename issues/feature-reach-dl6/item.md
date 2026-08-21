---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: done
priority: normal
epic: cheap-fast-analysis
labels: [dl6, reach, lane-e]
closed: 2026-08-21
closed_by: chris
commits:
- hash: e72d1088c
  summary: feature-reach.dl6, hafley-rs and sprefa matrices, reachcrate fixture
---

# feature-reach.dl6: entry point x feature matrix from extract output

## Description

cargo_targets roots + call sites + scip.call/scip.diet.call + df edges; reach closure with hop counts; matrix with via scip|diet|both|none; run on hafley-rs (boop subcommands as oracle) and on sprefa-extract; fixture crate with expected tsv. Lane E.
