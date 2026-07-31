# Language-agnostic CLI bench contract — brief (opus worktree)

Phase 0 of plans/2026-07-31-rust-course.md (read it first). This gate
precedes any rust lowering. Deliverable: one CLI contract any engine
implementation can satisfy, a harness that grades correctness + timing,
and first standings.

## Build-vs-buy FIRST (standing law)

Written candidate analysis before any bespoke harness line: hyperfine
(expected winner for the timing leg), plain `/usr/bin/time -l` loops,
bencher, anything else you find in-tree (bench/engines rig, PERF-REPORT.md
conventions, v6/tsv2/SCALE.md, src/bench). The correctness referee
(tick-log byte diff) is ours either way. Record the analysis in the
contract doc.

## Contract (draft, tighten as measured)

- Engine executable: `<engine> --program <path> --schedule <path>
  --db <path>` -> tick-log JSONL on stdout (item-9 format, canonical json
  per json_ticklog ruling), perf JSON to `--perf-out <path>` (wall_ms,
  ticks, statements, peak_rss_mb, db_bytes; N/A-with-reason columns per
  PERF-REPORT convention).
- Adapters to write NOW: tsv2 (wrap `bop run` or serve path — whichever
  already emits the tick log), swipl oracle (dl6_oracle door). v5 rust
  adapter only where a program is expressible both sides (flagship rig
  precedent) — if that is large, price it and stop.
- Referee: byte-diff each engine's log vs the oracle's for the same
  program+schedule. An engine without a log is DISQUALIFIED, never timed
  (the v1 asymmetry lesson).
- Standings: one CSV + rendered table over the existing scale fixtures
  (s1/s2/s3, DAG/CYC where expressible) + callgraph flagship. Same input
  hashes across engines.

## Receipts required

- `just bench-cli` runs the whole thing; standings table committed
  (bench/STANDINGS.md or similar).
- Correctness leg green for every timed row.
- The buy-verdict written with candidates and why.
- Battery untouched: conformance count unchanged (you add no fixtures);
  green not required, but `just conformance` before exit.

## Fences

- Touch: new bench-cli files (pick a clean dir, e.g. v6/bench-cli/),
  justfile recipe, the contract doc.
- Do NOT touch: compiler/runtime/serve code, existing bench rigs
  (read-only reference), any running lane's files (0_refusal_messages,
  registry/lower/emit, READINESS, clock check).
- git worktree, base sha stated at dispatch; verify with rev-parse FIRST,
  STOP AND REPORT on mismatch. Commit per step `git commit -n`; no push.
