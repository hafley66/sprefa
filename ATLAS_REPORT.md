# ATLAS_REPORT — perf-bench consolidation lane

Lane: `chore/perf-bench-atlas` (flash). Base sha verified at dispatch:
`e926a196`. This report records the `just perf-all` transcript, the
BENCHMARKS.md doc it backs, and the one bench that did not fully run.

## TOC

1. [What landed](#what-landed)
2. [perf-all transcript: headers + timings](#perf-all-transcript-headers--timings)
3. [BENCHMARKS.md TOC](#benchmarksmd-toc)
4. [Bench failures and skips](#bench-failures-and-skips)
5. [The commit](#the-commit)

## What landed

Two files, per the lane's ownership (bench internals, FACTS*, STANDINGS,
PERF-REPORT, budget.json, and sqlite_baseline were NOT touched):

- `v6/justfile` — the `perf-all` recipe (and `perf-all-deep` for the store
  rig's full ladder). One command runs every perf bench in the repo; each leg
  reuses its own recipe, echoes a ten-word purpose header, runs under a named
  run-capped budget (failure-modes class 38), and prints wall time.
- `v6/labs/BENCHMARKS.md` — the one doc explaining every bench's purpose,
  workload, bank, measured wall time, run command, and history with citations.

`just perf-all` was run once end to end at base `e926a196` and exited 0.

## perf-all transcript: headers + timings

```
=== ATLAS: every leg below is documented at v6/labs/BENCHMARKS.md ===
=== shootout: in-RAM rust engines, fixpoint derived rows/sec ===
    harness: run complete; standings written to .../STANDINGS.md
==> shootout wall 162s (exit 0)
=== dl6-bench-full: emitted prolog runtime, grid/layered/chain build ===
    grid_10000 3,960 | 1,069,200 | 9d7239568960d6a8 | 24 | 1298 | ...
    layered_10000 9,984 | 9,951,396 | addcf85b5162b9da | 4 | 12492 | ...
    chain_10000 7,743 | 9,996,213 | df09b2f409f8b9a8 | 5 | 28233 | ...
==> dl6-bench-full wall 164s (exit 0)
=== dl6-dred-bench: in-place DRed vs refCount, incremental ticks ===
    build_ms=1312 head=1069200
    insert_one_ms=29 delete_one_ms=60 drain_ms=1 delete_structural_ms=83
==> dl6-dred-bench wall 20s (exit 0)
=== dl6-budget: budget cell, grid fixpoint time + RSS ceilings ===
    budget grid_10000 fixpoint_ms 1326 <= 2500 OK
    budget grid_10000 peak_rss_mb 602 <= 900 OK
==> dl6-budget wall 4s (exit 0)
=== store-rig: hermetic engines, retract (small scale; perf-all-deep = full ladder) ===
    SKIP sqlite-mem/dd/dbsp (example binaries folded out at base sha)
    swi-incr/swipl-pure/swi-sqlite/swi-ts/swi-emit 2x200 OK
    TSV2_ORACLE_OK s1/1000 byte-identical; tsv2-gen 1x1000 OK; V1 s1 N/A
==> store-rig wall 4s (exit 0)
=== profile_dred: single cycle-safe retract, per-phase flame numbers ===
    retract wall 1793.2 ms; rounds 11; survivors/killed 800002/160000
    sqlite C-heap 86.6MB; process peak RSS 306.1MB
==> profile_dred wall 5s (exit 0)
PERF-ALL EXIT: 0
```

Total `perf-all`: ~6 minutes. The store rig was measured DEEP separately
(past 3 min still inside the swipl-engine segment at ~4 min before it was
stopped), which is what drove the decision to hand `perf-all` the rig's
smallest scale (~4s) and put the full ladder behind `just perf-all-deep`.

## BENCHMARKS.md TOC

The new `v6/labs/BENCHMARKS.md` opens with a truth-stack (dd-in-rust = true
ceiling; hand-rolled in-RAM rust engines = physics reference; pure sqlite =
disk-class middle; emitted dl6 runtime = the ratchet subject) then:

1. [The truth stack](#the-truth-stack)
2. [perf-all, the consolidated command](#perf-all-the-consolidated-command)
3. [rust shootout](#rust-shootout---in-ram-engines-build-throughput)
4. [dl6 emitted bench](#dl6-emitted-bench---the-ratchet-subject)
5. [dl6 retraction ticks](#dl6-retraction-ticks---in-place-dred-vs-refcount)
6. [store retraction rig](#store-retraction-rig---hermetic-engines)
7. [dred profile](#dred-profile---single-retract-flame)
8. [dl6 budget cell](#dl6-budget-cell---the-regression-gate)
9. [sqlite build baseline](#sqlite-build-baseline---landing)
10. [Open items](#open-items)

## Bench failures and skips

- All six perf-all legs passed (exit 0).
- The store rig SKIPs `sqlite_reach` / `dd_reach` / `dbsp_reach`: those example
  binaries were folded out of the crate at `a7d5ad36` (lab collapse) and do not
  exist at base `e926a196`. `bench/run.sh` still lists them. Reported in
  BENCHMARKS.md as an open item (restore the binaries or drop the entries).
- `sqlite_baseline` is absent at base `e926a196` (landing in a parallel lane);
  it is listed as "landing" in BENCHMARKS.md and excluded from the recipe until
  it lands.

## The commit

Implementation commit sha: `4599d727` (parent of this report's final amend).

- `v6/justfile` — `perf-all` + `perf-all-deep`
- `v6/labs/BENCHMARKS.md`
- `ATLAS_REPORT.md` (this file)

No push, no PR.
