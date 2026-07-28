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

## AMENDMENT (user, before launch): ride the EXISTING bench harness

Do not build a standalone harness. v6/sprefa-store/bench/ is the house
scale-test rig: one bash runner per engine in bench/engines/*.sh, each
emitting the shared 8-field CSV protocol (see pure_wrap.sh header),
orchestrated by run.sh with report.sh/chart.sh over bench/out/. Add tsv2
as ANOTHER ENGINE THERE:
- bench/engines/tsv2_gen.sh: bash runner following the existing shape
  (same CSV fields, /usr/bin/time -l RSS merge) that compiles a
  parameterized synthetic program via compile_fixture/4 and runs it on
  the tsv2 A runtime for the given size args.
- Reuse the harness's existing size parameterization style (layers/width
  args like pure_wrap.sh) mapped onto the ROWS x SHAPE matrix from this
  brief; if a shape does not fit the layers/width convention, add args,
  do not fork the protocol.
- Results land in bench/out/ through the existing report path; SCALE.md
  becomes a short pointer + the headline table copied from the report,
  not a parallel format.
- The oracle cross-check rail stays exactly as specified.
- scale-gen.pl may live in bench/ beside swi_reach.pl (the precedent for
  prolog helpers in the bench dir) instead of v6/tsv2/scripts/.

## AMENDMENT 2 (user, mid-run): baseline = the OLD TS engine, same matrix

The report must contain side-by-side rows: every matrix cell run through
BOTH (a) the tsv2 A-runtime path (tsv2_gen.sh) and (b) the existing v1
TS engine path that the harness already drives (the swi_emit.sh /
swi_ts.sh lineage: prolog emits the program, node runs it through the
v1 evalProgramSql seam -- the lowerSql DatalogEvaluator, i.e. the KNOWN
TS baseline). Both are recompute-per-tick engines (store-adoption
findings, plans/2026-07-28-store-adoption-findings.md), so this is a
fair same-class comparison; the delta isolates runtime overhead, not
algorithm class. If the v1 seam cannot host a matrix shape, record
N/A-with-reason for that cell rather than bending the shape.
(Queued for a codex resume if the first run lands before this
amendment is seen; the runner and matrix stay as specified.)
