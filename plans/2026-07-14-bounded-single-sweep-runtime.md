# Bounded single-sweep runtime

## Context

On 2026-07-14 a small `? calls => callee_name` invocation fell back from a
daemon that had not become ready within five seconds and started a second,
in-process engine. The foreground engine completed 107-file extraction while
the daemon was still replaying 5,791 files and 173 relations. The query was not
the expensive part: its derived `calls` phase took 9.9 ms. Concurrent cold
replay, uncapped Rayon work, and daemon readiness being coupled to replay
produced the observed roughly 500% CPU spike.

The immediate guard is now a two-thread global Rayon default (overridable with
`DL_RAYON_THREADS`). This plan addresses the underlying scheduling and memory
shape rather than treating that cap as the architecture.

The current engine already inventories each `(repo, rev)` once by grouping
coordinates and unioning their globs in `src/engine/reconcile.rs`. Extraction
then dispatches requested families serially in `src/engine/tick.rs`, and each
family independently obtains the file set and parses it through
`src/engine/extract/`. Rust type, call, and dataflow extraction already have
helpers that accept the same `syn::File`, so they are a controlled place to
test one-parse/many-projection extraction.

There are three independent resident-memory risks:

- the daemon's unbounded watcher channel can retain events while a tick runs;
- reconciliation and node extraction materialize multiple corpus-sized maps
  and vectors at once, while per-family fact caches remain resident;
- every SQLite connection currently requests a 512 MiB cache and 512 MiB mmap
  window and uses memory-backed temporary storage, so per-root engines can
  multiply a nominally global budget.

Historical v3 work is useful evidence, not code to transplant. Commit
`620c466a` documents corpus-level `ScanBatch`, SQLite batching, and Git
`cat-file --batch`; commit `15bb9bfb` has cached and batched reads. Its bounded
work-steal implementation only bounded the Tokio inbox before immediately
spawning onto Rayon, so it did not bound queued payloads or Rayon work.

## Decisions

1. Prove the extraction premise before changing the daemon. The first artifact
   is a Rust-only A/B micro-experiment comparing today's three family passes
   with one read, one `syn` parse, and three projections. TypeScript is a second
   language validation only after the Rust API and measurements pass.
2. Keep responsibilities explicit: Tokio coordinates durable intents and
   generation state; SQLite is the compact queue and recovery source of truth;
   Rayon performs bounded CPU extraction with a default of two workers.
3. Represent pending work as identities, not payloads. A queued job is
   `(coordinate, path, content_id, wanted_bits)`; file contents, syntax trees,
   emitted facts, and strata-specific copies never live in the intent queue.
4. Use one global inventory sweep per generation initially. Within that sweep,
   each changed content identity is read and parsed at most once per language,
   then all wanted extraction families project from the shared parse.
5. Keep strata metadata proportional to relation nodes and dependency edges.
   Files and fact bundles must not be copied per stratum. Global resolution
   starts after bounded local extraction reaches its generation barrier.
6. Commit a generation atomically. Stage bounded local results, apply
   retractions/inserts and affected strata in one root transaction, advance the
   root generation watermark, then acknowledge scheduler intents and dispatch
   effects. A separate WAL reader serves the last committed generation while
   the next one is staged.
7. mmap is optional transport for large immutable blobs, behind a measured
   threshold. It is excluded from the first A/B because it would confound the
   parse-reuse result and does not solve queue residency or stratification.

Rejected alternatives: revive the v3 general cursor queue (insufficiently
bounded); enqueue file bytes or syntax trees (resident memory grows with
backlog); run extraction on Tokio (CPU work harms orchestration); one SQLite
cache budget per root (unbounded process multiplier); and mmap every file
(page-fault overhead for small hot-source files without reducing logical live
data).

## Micro-experiment: one parse, many projections

Add `examples/extract_ab.rs` and `bench/extract-ab.sh`; do not route this spike
through production scheduling. Both arms inventory once and retain identical
emitted fact sets so the RSS comparison is fair.

- Baseline A: for each requested Rust family (`type`, `call`, `dataflow`), read
  and parse every file as current family dispatch does.
- Candidate B: read each file once, build one `syn::File`, and project a masked
  `AnalysisBundle { types, calls, dataflow }` using existing `_from(&syn::File)`
  helpers. A one-family mask must not allocate the other output vectors.
- Run exactly two Rayon workers. Generate deterministic valid-Rust fixtures at
  `N = 128, 512, 1024, 2048`, approximately 8–16 KiB per file. Alternate A/B
  seven times, discard the first run, and compare medians. Optionally repeat on
  a fixed rust-analyzer corpus snapshot after the synthetic rail passes.
- Emit JSON counters for files, source bytes, inventory sweeps, reads, parses,
  elapsed milliseconds, rows per family, checksum, maximum jobs in flight, and
  maximum input bytes in flight. Capture maximum RSS with `/usr/bin/time -l`
  on macOS or `-v` on Linux.

The experiment supports the design only if all of these hold:

- exact row counts and a stable content checksum match A;
- inventory sweeps are one in both arms;
- A performs `3N` reads/parses and B performs `N`;
- maximum concurrent jobs are at most two, and live input bytes are no greater
  than the sum of the two largest files;
- B median wall time is at most 75% of A (75–90% is inconclusive; above 90%
  refutes the expected CPU win);
- B maximum RSS is at most `max(A * 1.10, A + 64 MiB)`;
- B's RSS slope across N is at most 110% of A's, with no monotonic post-work
  growth beyond the deliberately retained fact payload.

Result on 2026-07-14: the experiment passed. Seven alternating runs were made
at every size; the first run was discarded for wall-time medians. Both arms
retained all emitted facts, which explains the deliberately steep absolute RSS
growth and makes the relative memory comparison fair.

| Files | A median | B median | B / A | A max RSS | B max RSS |
|---:|---:|---:|---:|---:|---:|
| 128 | 303.6 ms | 134.2 ms | 44.2% | 89.1 MiB | 83.2 MiB |
| 512 | 1198.8 ms | 536.2 ms | 44.7% | 314.9 MiB | 294.7 MiB |
| 1024 | 2397.5 ms | 1072.5 ms | 44.7% | 614.8 MiB | 575.8 MiB |
| 2048 | 4752.4 ms | 2134.3 ms | 44.9% | 1219.3 MiB | 1136.8 MiB |

All four semantic comparisons matched exactly. Across 56 timed runs, every
run used one inventory sweep, no more than two jobs, and no more live input
bytes than the two largest files. A performed `3N` reads/parses and B `N`.
Bundling therefore clears the CPU, peak-RSS, and RSS-slope adoption gates.

Production follow-through on 2026-07-14 primes the existing type/call/dataflow
fact caches from one Rust bundle in both full ticks and daemon/LSP path ticks.
It keeps the established per-family digest, resolver, row-write, and eviction
contracts; only facts survive the Rayon job. The extraction-cache integration
rail proves a two-file cold tick performs two physical parses rather than six,
and a one-file `tick_paths` edit performs exactly one additional parse while
all three relation families remain populated.

The production-corpus A/B used fresh isolated databases, the same 107-file
(2.71 MiB) `src/` corpus, and two Rayon workers. `DL_DISABLE_ANALYSIS_BUNDLE=1`
selected the separate-family control arm. Both arms produced exactly 100,180
cold rows and 100,189 post-edit rows with matching content digests.

| Production phase | Bundled | Separate | Result |
|---|---:|---:|---:|
| Cold wall | 7,383 ms | 7,744 ms | -4.7% |
| Cold physical parses | 107 | 321 | -66.7% |
| Warm wall / parses | 100.8 ms / 0 | 98.8 ms / 0 | equivalent |
| One-file `tick_paths` wall | 6,950.8 ms | 6,939.9 ms | equivalent |
| One-file physical parses | 1 | 3 | -66.7% |
| Peak RSS | 671,488 KiB | 679,216 KiB | -7,728 KiB (-1.14%) |

This validates bundling but also isolates the next bottleneck: a one-file
incremental tick takes nearly as long as the cold tick even after physical
parsing falls to one file. The next measurement gate must profile phase wall
time and relation invalidation volume inside `tick_paths`; repeating extractor
A/B runs will not explain that remaining cost. The opt-in ignored production
probe is `tests/it/analysis_bundle_ab.rs`.

## Runtime design

Use a small scheduler database under daemon state, separate from each root's
fact database. A capacity-one Tokio wake channel merely announces that durable
work exists; it is not the queue. The minimal durable shape is:

```text
root_intent(root_key PK, requested_seq, claimed_seq, full_work,
            reload_program, poll, priority, not_before, failures)
path_intent(root_key, seq, coordinate, path, wanted_bits,
            PK(root_key, coordinate, path))
coordinate_intent(root_key, repo, declared_rev, resolved_oid, priority,
                  wanted_bits, program_digest,
                  PK(root_key, repo, declared_rev, resolved_oid, program_digest))
```

Coalescing is monotone and cheap: repeated paths replace older content intent
and union `wanted_bits`; poll is a bit; reload replaces an older program
digest; a full marker subsumes path rows up to its claim watermark. Hitting a
configured path-row or encoded-byte cap promotes the root to full work and
deletes the subsumed rows. Events arriving during generation G remain durable
and cause at most G+1. Immutable Git OIDs can remain reusable for the active
program digest; moved branch names resolve to new coordinate intents.

The dispatcher claims a sequence watermark, streams inventory and bounded
file claims (initially 16–32 identities), and admits at most two extraction
jobs. Fact bundles also have a byte cap and large CST/node output is chunked to
staging. On restart, the root generation watermark distinguishes a committed
generation from scheduler acknowledgement lost after commit.

Priority tiers are hot working-tree/RPC work, poll/effect work, immutable Git
work, then full recovery, with aging so a cold tier cannot starve. Only one
global inventory sweep may run initially; this makes the CPU and memory budget
legible before considering safe per-root parallelism.

<!-- todo(feature): implement the durable coalescing intent store, generation watermark protocol, and capacity-one Tokio wakeup -->
<!-- todo(perf): replace corpus-wide extraction payload caches with bounded byte-weighted reuse and stream WORK and Git inventories into staging -->
<!-- todo(perf): replace per-connection 512 MiB SQLite cache and mmap settings with one measured process-wide budget and permit disk-backed large temporary work -->

## Verification

### Correctness and recovery

- Property-test coalescing for associativity, commutativity, and idempotence;
  include sequence watermarks, wanted-bit union, content supersession,
  path-cap promotion, lease restart, and immutable-OID reuse.
- Crash-inject before claim, during inventory, during staging, before and after
  the root transaction, after commit/before scheduler acknowledgement, and
  during effect dispatch. Restart must expose either the old or new committed
  generation, never mixed relation tables, and must not lose an intent.
- During an intentionally slow generation, a query must promptly read the
  previous committed generation. Continued edits during G produce exactly one
  follow-up generation, not an overlapping tick per edit.

### Queue and load tests

- 100,000 events for one path collapse to one path row and at most one
  follow-up generation.
- 100,000 distinct paths cross the configured limit and collapse to a full
  marker; in-memory queue usage remains constant with historical backlog.
- Polls collapse, priority aging prevents starvation, client disconnects do
  not discard durable work, and a moved Git branch schedules the new OID once.
- A 500-repository soak reports SQLite cache/mapped bytes, Rust heap, staging
  bytes, per-root idle cost, queue rows/bytes, and open content handles.

### Stratification amplification rail

Run the same fixed files and projection mask through programs with 1, 10, 100,
and 1,000 dependent strata. Parser calls, path-intent rows, open content
handles, staged fact bytes, and extraction peak RSS must remain constant within
measurement noise. Only planner metadata may grow linearly with the number of
relation/edge definitions. Fail if file payloads, parse trees, or fact bundles
are retained per stratum.

### Required runtime metrics and gates

```text
running_sweeps_global <= 1
rayon_workers <= 2                 # unless explicitly overridden
open_content_handles <= 2
path_intent_rows <= configured cap
parse_count(content, lang, generation) <= 1
generation is strictly increasing
queries observe exactly one committed generation
steady RSS has zero slope with event count after coalescing
extraction RSS is invariant to stratum count
queue heap is invariant to historical backlog
```

The rollout order is: land measurement counters; run the isolated A/B; land
the durable intent/coalescing store; stage bounded extraction; add atomic
generation commit and concurrent reader; then enable daemon routing. Each step
keeps a production fallback until its correctness, crash, and RSS rails pass.

<!-- todo(bug): decouple daemon socket readiness from cold replay so a healthy daemon does not trigger concurrent in-process fallback -->
<!-- todo(feature): add committed-generation reads during staging and the crash-injection integration matrix -->
<!-- todo(perf): add the 1-to-1000-strata RSS amplification rail and 500-repository steady-state soak -->
<!-- todo(perf): profile tick_paths phase time and relation invalidation volume because a one-file edit still costs approximately the full cold tick after bundled extraction -->

## Staffing

- Base SHA: `4ec7dc404c4e3392493269b928435ef26afef2e8`.
- Luna owns sharply specified implementation and test slices: counters,
  fixtures, the Rust A/B harness, coalescing property cases, and deterministic
  crash points. Each slice lands only with its named gate.
- Terra owns the harder cross-cutting changes: durable scheduler schema,
  generation/transaction protocol, concurrent-reader boundary, resident-memory
  accounting, and review of any mmap threshold.
- Worktree policy: one worktree per independently landing slice; do not let
  agents share modifications to engine scheduler or database files. Rebase on
  the base branch and run the slice suite before handoff.
- Suite budget: micro A/B under 15 minutes on synthetic fixtures; unit and
  property tests under 2 minutes; crash matrix under 10 minutes; 500-repo soak
  is an explicit long-running perf job and is not part of the edit loop.
