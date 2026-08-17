---
created: 2026-08-14
updated: 2026-08-15
type: chore
reporter: fable
status: done
priority: normal
closed: 2026-08-15
commits:
- hash: dbf37541
  summary: golden-flex coverage gate green -- 15/16 unaccounted constructs exercised, split/2 named-absent pending issuectl list-column-raw-snapshot-crash
---

# golden-flex coverage gate stale: 16 unaccounted registry constructs block regeneration

_Source: v6/tsv2/scripts/golden-flex.sh_

## Description

just golden-flex fails at stage 1 (coverage gate, golden_coverage.pl): 16 registry constructs unaccounted (split/2, substr/2, substr/3, trim/1, trim/2, upper/1, and 10 more from the string-family landing). CI-KNOWN-RED.md:22 still records the old 2-unaccounted state. Because the gate front-runs the emit step, gen_emitted/golden-flex.ts goes stale against runtime types and just typecheck rots (it sat at Aug-11 output, causing 2 phantom type errors). Fix: exercise the new constructs in v6/dl/fixtures/golden-flex.dl6 or record named absences in expected_absent/2 + the golden header; then refresh the CI-KNOWN-RED entry.

## Resolution

### 2026-08-15T04:05:33Z · @issuectl

just golden-flex measured green 3/3 on worktree-agent-acd35a3581b5722e2 base dc97a827: coverage gate (83 constructs = 71 exercised + 12 named absences), text door, all four cardinality/mode-parity legs (zero/one/many/perturbed), served e2e. just typecheck 0 errors after full just sweep regen. CI-KNOWN-RED.md's golden-flex row and allowlist entry removed. split/2 is the one construct NOT exercised: its result type is unavoidably list(T), and any live list(T)-typed row crashes tsv2/runtime/rows.ts row_value_from_sql via read_stored_snapshot's raw (non-view) SELECT -- not split-specific, the pre-existing tree_bundle: list(patch) column has the identical shape and was simply never fed a row by golden-schedules.ts. Filed as issuectl list-column-raw-snapshot-crash rather than patched (runtime/emit files are out of this issue's ownership).
