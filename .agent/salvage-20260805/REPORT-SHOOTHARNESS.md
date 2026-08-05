# shootharness — defect fixes report

Three exec_shootout harness defects fixed on `lab/shootharness`, one commit.

## Defect 1: exit 0 on engine failure

Fix: `v6/labs/exec_shootout/harness/src/main.rs` — `best_of_three` now returns
`(Option<RunOutcome>, bool)` instead of calling `std::process::exit(1)` on a
failed run. A failed engine becomes a DNF row, sets `had_failure`, and the run
continues. STANDINGS is always written with the failure recorded; after writing,
the process exits nonzero when `had_failure` is set. DNF rendering added to
`src/standings.rs` (`EngineRow::dnf`, derived/throughput columns show "DNF").
Exit-path test added at `src/runner.rs` `reports_nonzero_child_exit`.

Note: checksum mismatch keeps the pre-existing immediate `exit(1)` with no
STANDINGS write, per CONTRACT.md:105 ("no standings are written from a run with
a mismatch"). It already exits nonzero and is excluded from the
write-standings deferral for that reason.

## Defect 2: builds table repeats

Fix: `v6/labs/exec_shootout/harness/src/main.rs` — the `--measure-builds` block
moved out of the per-family/per-scale case loop (was running once per case = 9x)
to a single pass over engines before the case loop. Builds are measured once per
engine, so the STANDINGS builds table prints each engine once.

## Defect 3: grid tuner scale-blind

Fix: `v6/labs/exec_shootout/harness/src/tuner.rs` — `tune_grid(scale)` now
derives a per-scale target derived-row count via `grid_target_derived`, log-
interpolating the CONTRACT band [1M, 20M] across the 10k..1M scale ladder, and
picks the square side closest to that target. Unit test
`grid_tuner_is_scale_aware_and_in_band` asserts the tuned params for 10k/100k/1M
are pairwise distinct and each derived count lands within the CONTRACT band.

Tuned grid sides: 45 (10k), 65 (100k), 94 (1M) — derived 1,069,200 / 4,596,800 /
19,927,389, all within [1M, 20M].

## Validation receipts (verbatim)

```
cargo test
```

```
test refengine::tests::semi_naive_stops_on_empty_delta ... ok
test refengine::tests::bitset_counter_matches_reference_on_dag ... ok
test refengine::tests::checksum_matches_hand_computed_on_three_edge ... ok
test refengine::tests::checksum_is_order_independent ... ok
test refengine::tests::semi_naive_terminates_on_cycle ... ok
test runner::tests::parses_three_events ... ok
test runner::tests::rejects_foreign_line ... ok
test runner::tests::rejects_missing_event ... ok
test runner::tests::reports_nonzero_child_exit ... ok
test tuner::tests::grid_tuner_is_scale_aware_and_in_band ... ok

test result: ok. 10 passed; 0 failed; 0 ignored
```

```
cargo build --release
```

```
Compiling exec-shootout-harness v0.1.0
Finished `release` profile [optimized] target(s) in 0.70s
```

## Live failure receipt

Broken engine `/usr/bin/false`, single scale 100000, no full ladder.

```
$ ./target/release/harness --engines /usr/bin/false --scales 100000 --work /tmp/xh_work --standings /tmp/xh_work/STANDINGS.md
engine run failed for /usr/bin/false: engine exited nonzero: ExitStatus(unix_wait_status(256))
... (x9)
harness: run complete; standings written to /tmp/xh_work/STANDINGS.md
harness: one or more engine runs failed; exiting nonzero
$ echo $?
1
```

STANDINGS.md still written; each engine row shows DNF. Grid case shows
`rows=65 cols=65` (scale-aware), builds table lists `false` once.
