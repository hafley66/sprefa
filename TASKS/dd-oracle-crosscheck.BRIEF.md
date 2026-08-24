# dd-oracle-crosscheck

Goal: the actual differential-dataflow ecosystem executes a panel of our conformance programs and its per-tick output delta stream is diffed against our oracle's expected deltas. Correctness only, no perf claims. Base: worktree at or past ef858d1a9411f73aca44d975c1985c9c7d58dcbd. Branch `feature/dd-oracle-crosscheck`. PR to main.

## Step 1, library pick with receipts (build-vs-buy law, no one-line dismissals)
Candidates: `differential-dataflow` (McSherry; multi-dimensional timestamps, arrangements) and `dbsp` (Feldera; step()-per-batch Z-set circuits). Write the comparison INTO the PR body: batch-per-tick fit, retraction spelling, distinct/negation/aggregate operator availability, dependency weight, MSRV against our toolchain. Pick one; dbsp's step model maps 1:1 onto our tick and is the expected winner, but the receipts decide.

## Step 2, fixture panel export
A small prolog exporter (new file under `v6/prolog/conformance/`, additive) dumps a JSON panel: program name, rel schemas, tick schedule (signed rows), expected per-tick deltas, expected finals. Panel = the 3 `test/dd/*` programs plus one representative each: two-way join, negation (`callgraph_unused_inverts_with_the_call_set`), recursion (`ordered_level_fixpoint` family), aggregate (`float_avg_is_grouped`), coalesce (`coalesce_defaults_the_absent_row`), retraction cascade (`recount_retraction_reaches_two_heads_same_tick`), distinct/multi-derivation. ~10 programs total. Fixture sources: `v6/prolog/conformance/fixtures/`.

## Step 3, the Rust harness
New test crate or test file in `v6/sprefa-engine-rs` (dev-dependency only; the engine's own build must not gain the dependency). For each panel program, HAND-BUILD the equivalent circuit in the chosen crate (no generic datalog interpreter; ~10 hand circuits keyed by program name), feed the schedule as signed batches, collect per-tick output deltas, and assert set-with-signs equality against the exported expectations. Sequence/ordering columns are ours, not dd's: compare as multisets of (row, sign) per tick per rel.

## Receipts
- The panel test red when a hand circuit is wrong: include one sabotage receipt in a test header (flip a sign, show the diff fires).
- All 10 programs: dd stream == oracle stream at every tick.
- Full gate green: conformance 445/0, plunit 1088/0, grade 445/341, cargo (now with the new tests)/0, ghcache ticks=14, goldens 6, ARCH 7/0. Ledger entry (next free number, renumber on collision). ARCH row `dd_oracle_crosscheck`.

## You own
new prolog exporter file, `v6/sprefa-engine-rs` tests + Cargo.toml dev-dependencies, `docs/failure-modes.md`, one ARCH row. Forbidden: `lower.pl`, `incremental.rs` and all engine src, existing fixtures, `v6/dl/**`.

## Style laws (CLAUDE.md)
No eprintln (tracing only), comment budget, no em dashes, banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). Batteries in background with timeout; no foreground wait over 10 s. Commit per step; PUSH before reporting.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: panel N/N green, numbers"`. Blocked: one line, stop.
