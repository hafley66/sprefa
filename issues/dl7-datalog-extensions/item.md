---
created: 2026-08-29
updated: 2026-08-29
type: epic
owner: codex
status: open
priority: high
labels: [dl7, comptime]
---

# DL7 stratified Datalog extensions

## Description

Supply the three checked-Datalog capabilities required by userland Pick and Exclude while preserving one shared evaluator.

## Scope

Relational list destructuring, negative goals over completed lower strata, and count aggregation. Source syntax remains prefix functional forms. Checked IR remains target-neutral. SQLite and every other emitter stay outside this epic.

## Acceptance Criteria

- [ ] Reversible cons supports bounded list traversal.
- [ ] Negative goals carry explicit polarity and stratify.
- [ ] Count aggregates read completed lower rows.
- [ ] Pick and Exclude become ordinary prelude rules.
- [ ] Compiler and runtime callers retain one evaluator entry point.

## Tests Run

- [ ] Existing consolidated V7 SWI suite.

## Implementation Notes

DL6 donors: v6/prolog/strat.pl and v6/prolog/0_compiler_relations/1_aggregates.pl.
