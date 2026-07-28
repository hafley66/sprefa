# CODEX BRIEF: tsv2 scale bench, first data (luna-class)

There are ZERO scale measurements for tsv2-generated programs. This brief
produces the first table. No thresholds, no pass/fail on numbers: the
deliverable is DATA plus a reproducible harness. Grades are about the
harness being honest, not the numbers being pretty.

## Build

New files only:
- v6/tsv2/scripts/scale-bench.ts (the harness)
- v6/tsv2/scripts/scale-gen.pl (synthetic program generator, prolog,
  emits fixture-term programs compiled via the EXISTING
  compile_fixture/4 path -- never hand-written gen files)
- v6/tsv2/SCALE.md (the results table, committed)
Nothing else. Do not edit the compiler, runtime, or fixtures.

## The matrix (each cell = one generated program run on the A runtime)

Dimensions, small grid, ~12 cells total:
- ROWS: EDB arrival volume per rel: 1k / 10k / 100k rows.
- SHAPE: (s1) one edge rule keyed replace; (s2) a 3-deep level-rule
  chain (a <- b <- c joins); (s3) the unmarked 2-atom combine trigger
  (the backlog-replay shape, worst case for late subscribers).
- Ticks: batches of 100 arrivals per tick so tick count scales with ROWS.

Per cell, measure with plain hrtime around the existing tickLoop (use
the DL_PERF_LOG channels if convenient, but do not add new trace code):
total wall, mean tick ms, p95 tick ms, max tick ms, final table sizes,
and ms-per-1k-arrivals. Emit one markdown table into SCALE.md plus the
raw JSONL beside it (gitignored if large; state the choice).

## Honesty rails

- The generator must print each generated program's term form into the
  bench output dir so a human can read what was actually run.
- Each cell runs on a FRESH in-memory or scratch-file db (state which
  and why; never a shared db across cells).
- One warmup run per cell discarded, one measured run recorded. Note
  machine load caveats in SCALE.md header.
- Cross-check one small cell (1k, s1) against the oracle: run
  ticklog.pl on the same generated program and diff logs byte-for-byte.
  A scale harness that diverges from the oracle at 1k is measuring a
  different engine; STOP if it diffs.

## Grades

1. Harness runs the full matrix end to end via one command
   (`node --experimental-strip-types scripts/scale-bench.ts` or the
   package's existing script style -- match it).
2. The 1k/s1 oracle cross-check is byte-identical.
3. Existing suites untouched and green: tsv2 6/6, import gate OK,
   conformance 110 (you touched none of their inputs).
4. SCALE.md contains the full matrix table with units, the machine
   line (chip, ram, node version), and the v5 yardstick row quoted for
   context (7,244 files/s ingest -- different workload, stated as
   NOT directly comparable, context only).

## Laws

Descriptive names; no em dashes; banned words provenance, substrate,
load-bearing, regime. One logical step per commit, git commit -n, no
push, no merge. If a cell OOMs or exceeds 10 minutes, record DNF for
that cell with the observed failure and move on; do not shrink the
matrix silently. Final summary: the SCALE.md table verbatim, DNFs,
oracle cross-check receipt, suite results.
