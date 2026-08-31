---
created: 2026-08-29
updated: 2026-08-29
type: epic
owner: codex
status: done
priority: high
labels: [dl7, comptime]
closed: 2026-08-29
closed_by: codex
commits:
- hash: d2d7410c0
  summary: Complete chained type operator rounds
---

# DL7 stratified Datalog extensions

## Description

Supply the three checked-Datalog capabilities required by userland Pick and Exclude while preserving one shared evaluator.

## Scope

Relational list destructuring, negative goals over completed lower strata, and count aggregation. Source syntax remains prefix functional forms. Checked IR remains target-neutral. SQLite and every other emitter stay outside this epic.

## Acceptance Criteria

- [x] Reversible cons supports bounded list traversal.
- [x] Negative goals carry explicit polarity and stratify.
- [x] Count aggregates read completed lower rows.
- [x] Pick and Exclude become ordinary prelude rules.
- [x] Compiler and runtime callers retain one evaluator entry point.

## Tests Run

- [x] Existing consolidated V7 SWI suite.

## Implementation Notes

DL6 donors: v6/prolog/strat.pl and v6/prolog/0_compiler_relations/1_aggregates.pl.

## Decisions

### 2026-08-29T22:43:52Z · @codex

Kernel capabilities are complete through count aggregation. The remaining Pick and Exclude criterion is blocked by @dl7-edge-snapshot-ruling because relation-level stratification rejects the strict ':' read/write cycle without a frozen source-edge boundary.

## Resolution

### 2026-08-29T23:39:08Z · @codex

The extension epic now includes reversible cons, stratified negation, count aggregation, ordered predecessor rows, compiler edge snapshots, and userland Pick and Exclude through the shared evaluator.
