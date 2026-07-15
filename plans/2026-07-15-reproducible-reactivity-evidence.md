# Reproducible reactivity evidence

## Context

Bundled Rust extraction already reduced a one-file path tick from three physical
parses to one, but the release measurement remained effectively unchanged:
6,950.8 ms bundled versus 6,939.9 ms separate. The cold tick was 7,383 ms. The
existing analysis therefore isolates the next problem: parsing is no longer the
dominant explanation; relation invalidation, derived evaluation, or SQLite work
still has nearly corpus-wide blast radius
([bounded runtime plan](2026-07-14-bounded-single-sweep-runtime.md#L132)).

The current `just bench` surface runs cold and warm corpus benchmarks through
`bench/run.sh`; the local and real-kernel recipes use the same runner
([justfile](../justfile#L42)). These are useful throughput checks, but they do
not demonstrate that one-file edit work stays flat while unrelated files grow.
The current performance log already records tick phase, parse/write profiles,
changed relations, derived strategy, fallback reason, and total time
([perflog](../src/perflog.rs#L140)).

The ownership/delta plan defines the target scaling evidence: one changed
owner, one parsed/projected file, no unrelated canonical writes, work
proportional to the changed-file fact difference, and flat one-file edit work
from 10 through 1,000 unrelated files. It also reserves the installed-release
corpus measurement until deterministic fixtures and equivalence rails pass
([delta plan](2026-07-14-delta-reactivity-and-fact-ownership.md#L1010)).

This plan turns those requirements into one safe, repeatable command and an
immutable evidence artifact before changing more runtime architecture.

### Scrappy call-graph micro-baseline (release, two workers)

The first implementation was deliberately reduced to a thin probe after the
general evidence harness became disproportionate. Engine-owned tick records
from deterministic repo-local fixtures are:

```text
files    cold    unchanged    one-file edit    clean rebuild    edit parses
10       35 ms   13 ms        14 ms            23 ms            1
100      39 ms   15 ms        21 ms            37 ms            1
1000     375 ms  35 ms        73 ms            192 ms           1
```

The edit stays on the structured incremental path, takes no fallback, parses
one file, and exactly matches a fresh rebuild's resolved call graph. It is not
flat with unrelated files. The leading code-level explanation is now narrow:
`refresh_call_rels` reuses per-file parsed facts, but reconstructs the
corpus-global definition/resolution indexes and refreshes the whole call-family
tables. The next slice measures and deltas that family; it does not add more
harness infrastructure.

Immediate call-family slice:

1. Time `cached_facts`, global name-index construction, resolution, and each
   relation write separately on the existing 10/100/1,000 probe.
2. Persist definitions and sites by file owner so one edit retracts/inserts one
   owner's rows instead of replacing the whole family.
3. Re-resolve only callees whose name bucket changed, callers in the edited
   file, and ambiguous/import-dependent buckets; keep a loud full-family
   fallback for unsupported cases.
4. Re-run the same probe. Accept only exact rebuild equality, one parse, no
   fallback, and near-flat edit work. Tests remain rails, not proof.

## Decisions

1. **Measure before changing execution.** The first landing adds the harness,
   fixture, counters, and baseline only. It does not route production ticks
   through new ownership/delta code.
2. **Use deterministic generated corpora.** The inner loop uses 10, 100, and
   1,000-file fixtures under an isolated repository-local scratch root and
   database beneath `target/reactivity/`. It never
   scans the sprefa workspace, a user repository, or the Linux kernel.
3. **Separate build from measurement.** `just perf-reactivity-build` builds the
   release example harness with two Cargo jobs. The example target preserves
   Cargo.toml's one-published-binary contract. `just perf-reactivity` never
   silently builds and fails clearly when the expected release executable is
   absent.
4. **Run in-process and daemon-free.** The harness constructs `Engine` directly
   and never calls the production CLI, configuration loader, watcher, or daemon.
   One incremental engine performs cold, unchanged, and edit ticks; it is
   dropped before a fresh database and engine perform the clean rebuild.
5. **Record work, not wall time alone.** Each run records exact work counters,
   phase time, wall/CPU/RSS, database/WAL/temp bytes, semantic digests, fixture
   identity, binary identity, and environment metadata as JSON.
6. **Treat equivalence as a gate, not a speed statistic.** The incremental
   result must equal a clean rebuild byte-for-byte or by a canonical semantic
   digest before performance comparisons are accepted.
7. **Make artifacts immutable and comparable.** Raw run JSON is named by Git
   SHA, program digest, fixture seed, and size. A deterministic report compares
   baseline and candidate artifacts without rerunning either workload.
8. **Optimize one vertical slice after measurement.** The first runtime change
   targets the measured dominant phase and one representative relation family;
   it does not attempt an engine-wide delta conversion.
9. **Walk through every stopping point.** No production corpus or next runtime
   slice begins until its prior artifact and interpretation are shown to the
   user.
10. **Forbid silent full fallback.** A structured path-tick report records the
    actual execution kind, fallback reason, and scope. The benchmark fails when
    the requested incremental scenario takes any full fallback.

Rejected alternatives:

- **Repeat extractor A/B:** already proved parse bundling and cannot explain the
  remaining 6.94-second derived cost.
- **Benchmark the workspace first:** risks freezing the user's machine and
  confounds correctness, scale, and local workspace state.
- **Use wall time as the gate:** cache warmth can hide full scans; exact work
  counters and query plans must remain flat too.
- **Convert all relations at once:** prevents attribution and creates too large
  a correctness and rollback surface.
- **Adopt another database for nested/performance features:** the evidence loop
  must exercise the existing storage seam and SQLite implementation.

## Evidence contract

One invocation produces one JSON document per `(scenario, fixture_size)` and an
environment manifest. Required identity fields:

```text
git_sha, dirty_worktree, binary_digest, rustc_version, target_triple
os, architecture, cpu_count, configured_cargo_jobs, configured_rayon_workers
fixture_seed, fixture_files, fixture_bytes, program_digest, database_path_kind
```

Required scenario fields:

```text
scenario = cold | unchanged | one_file_edit | clean_rebuild
generation, wall_ms, cpu_ms, peak_rss_bytes, rss_measurement_kind
database_bytes, wal_bytes, temp_bytes
inventory_sweeps, files_read, files_parsed, files_projected
source_rows_added, source_rows_removed
derived_rows_added, derived_rows_removed
changed_relations, invalidated_relations
sql_statements, sql_rows_read, sql_rows_written, sql_page_visits
fallback_count, fallback_reason, fallback_scope
semantic_digest, result_rows
```

Unavailable counters must be emitted as `null` with a named reason in the first
baseline. They must not be invented from wall time or silently omitted. The
counter inventory decides which missing counters are required before the
baseline is accepted.

All runs use:

```text
CARGO_BUILD_JOBS=2
DL_RAYON_THREADS=2
daemon disabled
one benchmark process at a time
isolated repository-local root/database under target/reactivity
fixed fixture seed and program
the Rayon global pool is initialized first and asserted to contain exactly 2 workers
TMPDIR and SQLITE_TMPDIR are confined beneath the same repository-local scratch ancestor
```

`peak_rss_bytes` is not accepted as a portable per-phase measurement merely
because `getrusage` exposes a process high-water mark. The first harness either
samples current RSS during each phase and names that method in
`rss_measurement_kind`, or records `null` with a reason. It does not relabel a
process-lifetime maximum as scenario-local memory.

Semantic equality covers a checked-in, explicitly named set of observable
relations. Each digest is computed from canonical relation identity, typed
column schema, and sorted typed rows. Database-file equality and a single
terminal query are not substitutes for this comparison.

## Sequence and stopping points

### Stopping point 1 — safe harness and baseline

<!-- todo(perf): add the deterministic 10/100/1,000-file reactivity fixture, daemon-free release harness, and separate no-build/build just recipes -->

<!-- todo(perf): inventory existing phase/profile/work counters and add only the minimum missing counters required by the baseline evidence contract -->

<!-- todo(perf): add a structured path-tick evidence result with actual execution kind and a benchmark policy that fails instead of silently taking a full fallback -->

<!-- todo(perf): capture immutable cold, unchanged, one-file-edit, and clean-rebuild JSON artifacts without running dl on the sprefa workspace or any production corpus -->

Deliverables:

- `bench/reactivity/` fixture generator, harness runner, schema/README, and
  deterministic report command.
- `just perf-reactivity-build` and `just perf-reactivity`.
- Baseline artifacts for 10, 100, and 1,000 files.
- A short walkthrough answering where the one-file edit spends time and which
  work counters grow with unrelated files.

Exit criteria:

```text
changed files = 1
parsed files = 1
actual tick kind = incremental
fallback count = 0
fixture and semantic digests repeat across identical runs
incremental semantic digest = clean rebuild semantic digest
the harness never starts a daemon or scans outside its repository-local scratch root
```

### Stopping point 2 — dominant-phase diagnosis

<!-- todo(perf): attribute the one-file edit to exact derived relations, SQL statements/plans, invalidation scope, and fallback boundaries, then select one representative vertical slice -->

No engine behavior changes in this step. The report names the dominant phase,
the relations and statements responsible, their scaling from 10 to 1,000
files, and the first relation family to convert. If the data contradicts the
current derived-invalidation hypothesis, revise the target rather than forcing
the ownership design into the hot path.

### Stopping point 3 — one owner/delta vertical slice

<!-- todo(perf): route one measured relation family through owner-scoped source deltas and affected derived maintenance while retaining a loud production fallback -->

The slice must cover edit, delete, rename, duplicate fact ownership, and final
owner retraction. It publishes a new candidate artifact using the exact same
harness and fixture identities as stopping point 1.

Exit criteria:

```text
incremental output = clean rebuild output
unrelated canonical rows written = 0
SQL statements/page visits stay flat from 10 to 1,000 unrelated files
one-file edit work stays within 20% from 10 to 1,000 unrelated files
fallback is zero for the supported slice and attributed elsewhere
RSS and staged bytes do not grow with unrelated file count
```

### Stopping point 4 — reproducible presentation

<!-- todo(feature): project benchmark artifacts into typed node/edge and table relations so sprefa can visualize its own phase, scale, and baseline/candidate history -->

Generate, do not hand-author, a Markdown comparison containing the fixture
matrix, phase attribution, work counts, semantic equivalence result, RSS, and
database growth. The same artifacts should be queryable through sprefa's
standard table/graph projection seam when that renderer surface is ready.

### Stopping point 5 — explicitly approved release gates

Only after deterministic fixtures pass:

1. Run the installed release on the isolated 107-file representative corpus.
2. Run a deterministic medium generated corpus.
3. Run the real Linux-kernel recipe as a separate, explicitly approved job.

Each release gate records the exact command, binary digest, machine manifest,
input revision, configuration, raw JSON, and generated comparison report.

## Verification

Harness verification:

- Run fixture generation twice and compare file manifests and digests.
- Run the 10-file scenario twice and compare semantic and work-counter fields;
  timing may vary and is reported as samples, never rewritten as exact.
- Assert every opened/scanned path is under the harness repository-local scratch root.
- Reject symlinks and verify the canonical fixture root, database, WAL,
  performance log, `TMPDIR`, and `SQLITE_TMPDIR` share one repository-local
  scratch ancestor beneath `target/reactivity/`.
- Assert daemon startup/attachment is impossible in the harness code path.
- Initialize the Rayon global pool before engine construction and assert the
  observed worker count is exactly two; do not ignore an already-initialized
  pool with a different width.
- Assert a missing release harness produces a clear no-build error.
- Validate every JSON artifact against the checked-in schema.

Correctness rails:

- One-file edit and clean rebuild produce identical canonical output.
- Delete retracts the final owner exactly once; duplicate ownership preserves a
  public fact until its last owner disappears.
- Rename is one owner removal plus one owner insertion in one generation.
- Unsupported operators take an attributed fallback and still match rebuild.

Performance evidence:

- Report cold, unchanged, edit, and rebuild separately.
- Report raw samples plus median and tail; never call tests proof.
- Compare work counters before timing conclusions.
- Fail scaling when SQL/page/invalidation work grows with unrelated files even
  if cached wall time looks flat.
- Initial edit target is under 250 ms, then under 100 ms on the isolated
  107-file release fixture; these are gates, not claims before measurement.
- End-to-end RSS target is 128 MiB on the 3,000-file generated fixture after
  the 10/100/1,000 evidence is stable.

Commands permitted during stopping points 1–4:

```text
cargo fmt --check
CARGO_BUILD_JOBS=2 cargo test <named bounded target>
CARGO_BUILD_JOBS=2 cargo build --release --example reactivity_harness
just perf-reactivity                 # generated fixture only, no build
```

Forbidden without explicit approval:

```text
production dl invocation
daemon/watch startup
sprefa workspace corpus scan
real repository or Linux-kernel benchmark
unbounded cargo test/build concurrency
```

## Staffing

- Base SHA: `2b10fbd6159b786ef008a6e3d48698821dd44c4b`.
- Root agent owns plan interpretation, task boundaries, integration, stopping
  point walkthroughs, and any engine/runtime design decision.
- A bounded worker owns `bench/reactivity/` fixture/harness/report plumbing and
  the two `justfile` recipes. The task is mechanical, may not edit engine code,
  and may not run production `dl` or corpus benchmarks.
- A bounded worker owns the counter inventory and a written gap matrix first.
  Any subsequent counter edit is separately assigned by file and field after
  root review; it may run only named unit tests.
- A review worker checks determinism, path confinement, daemon exclusion,
  artifact schema, and whether the harness can accidentally build or scan the
  workspace. It begins read-only and does not duplicate implementation.
- Model-tier selection is not exposed by the current collaboration runtime;
  delegated tasks are nevertheless written for low-judgment execution, with
  integration and cross-cutting decisions retained by root.
- Agents share the current workspace, so assignments use non-overlapping files.
  No agent edits the dirty engine files unless root assigns an exact counter
  slice after inspecting user changes.
- Suite budget: harness unit checks under two minutes; the 10/100/1,000 generated
  fixture run under five minutes; one process and two workers maximum.
- Formatting runs once at the integration stopping point, immediately before a
  commit, consistent with repository policy.
