# BENCHMARKS — every perf bench in the repo, one command, one explanation

Every perf benchmark in this repo now runs behind ONE justfile command:

```
cd v6 && just perf-all        # the whole inventory, small store-rig scale
cd v6 && just perf-all-deep   # same, store rig at its full scale ladder
```

A bench that is not in this list does not count. A bench in this list but not in
`just perf-all` is a defect. This document is the source of one-line purpose,
history, and the bank each bench writes its numbers to.

## TOC

1. [The truth stack](#the-truth-stack)
2. [perf-all, the consolidated command](#perf-all-the-consolidated-command)
3. [rust shootout](#rust-shootout---in-ram-engines-build-throughput)
4. [dd_plan rust arms](#dd_plan-rust-arms---correctness-only-throughput-gap)
5. [dl6 emitted bench](#dl6-emitted-bench---the-ratchet-subject)
6. [dl6 retraction ticks](#dl6-retraction-ticks---in-place-dred-vs-refcount)
7. [store retraction rig](#store-retraction-rig---hermetic-engines)
8. [dred profile](#dred-profile---single-retract-flame)
9. [dl6 budget cell](#dl6-budget-cell---the-regression-gate)
10. [sqlite build baseline](#sqlite-build-baseline---landing)
11. [shared frontier arms](#shared-frontier-arms--per_rel-vs-frontiershared)
12. [Open items](#open-items)

## The truth stack

One frame for reading every number in this file, the exact hierarchy the bench
divisions encode:

- **dd-in-rust is the true ceiling.** The resident differential-dataflow oracle
  (`src/oracle.rs`, `dd_reach`) is the correctness oracle and the
  resident-RAM speed yardstick; nothing on disk is expected to beat it.
- **Hand-rolled in-RAM rust engines are the physics reference.** The
  shootout's interp/rxgraph/mono engines bound what a bespoke in-memory
  engine can do for one build fixpoint.
- **Pure sqlite is the disk-class middle.** The store rig's sqlite strategies
  and the landing `sqlite_baseline` occupy the class the shipped runtime has
  to beat.
- **The emitted dl6 runtime is the ratchet subject, traced per-statement into
  the gap.** The dl6 emitted bench and dl6-budget hold the emitted runtime to
  ceilings that ratchet DOWN only.

## perf-all, the consolidated command

Defined in `v6/justfile`. One battery, ordered so the cheap gate (dl6-budget)
runs and cheap build-oriented legs come before the heavier retraction rig:

```mermaid
flowchart LR
  S[shootout] --> D1[dl6-bench-full]
  D1 --> D2[dl6-dred-bench]
  D2 --> B[dl6-budget]
  B --> R[store-rig]
  R --> P[profile_dred]
```

Every leg reuses its own recipe (`just shootout`, `just dl6-bench-full`, ...),
echoes a ~ten-word purpose header, runs under a named run-capped budget
(failure-modes class 38, `docs/failure-modes.md:1388`), and prints its own wall
time. A failing leg is echoed with its exit status and the batch continues, so
a broken bench is reported and not silently dropped.

The store rig is the one bench with two depths. Its full ladder
(`bench/run.sh` defaults: SCALES `2x200`..`14x80000` plus the 9-cell TSV2 and
V1 ladders) runs well past three minutes — measured on the dev machine it was
still inside the swipl-engine segment, with the expensive TSV2/V1 ladders yet
to come, when it passed ~4 minutes and was stopped. `perf-all` therefore
drives the rig at its SMALLEST scale (~4s); the full ladder lives at
`just perf-all-deep`.

Measured wall (this lane, `e926a196`, one machine, 2026-08-10, `just perf-all`)
is per-leg in the sections below; the consolidated transcript headers and
timings land in ATLAS_REPORT.md at the worktree root.

---

## rust shootout — in-RAM engines, build throughput

**Purpose.** What does a hand-rolled in-memory rust engine do for one closure
build, expressed as derived rows/sec in the fixpoint phase? This is the
physics-reference tier: the cheapest way to compute the answer with no disk
and no incremental machinery.

**Workload + engines.** Three rust engines (interp, rxgraph, mono) over three
shapes (chain, grid, layered) at 10k / 100k / 1M scales, `--measure-builds`,
best of 3. A reference engine emits the answer the standings compare against.

**Where numbers bank.** `v6/labs/exec_shootout/STANDINGS.md`.

**THE number.** Derived rows/sec in the fixpoint phase, best of 3
(`STANDINGS.md:5`). The headline is the mono engine at chain 10k, ~6.8e7
rows/sec; the single best number per scale/class is the bolded `best (THE
number)` row.

**Run command.** `cd v6 && just shootout`

**Expected wall time.** ~162s measured this lane (includes the four cached
release builds; a cold build adds first-compile time).

**History.** The shootout predates the v6 store; it established the in-RAM
ceiling the emitted runtime is read against. The `STANDINGS.md` THE-number
convention is the citation for every `~7e7`-class claim about the fixed build
fixpoint.

---

## dd_plan rust arms — correctness only, throughput gap

**Purpose.** The prolog compiler has three emitter outputs: tsv2, and the two
dd_plan arms of `v6/dd-runner`. This row exists because the user asks that
every emitter output be benched, and these two arms are the measured gap: they
have a correctness gate but no throughput bench. The arms are named for what
they are (diet = dd-shaped without the algebra):

| arm | what it is | flags |
|---|---|---|
| `dd-diet-rust-sqlite` | rust + rusqlite, executes the tick phases against SQLite | `--dd-diet-rust-sqlite` (default), formerly `--sqlite` |
| `dd-diet-rust-rust` | rust + hand-written in-RAM evaluator, zero SQLite | `--dd-diet-rust-rust`, formerly `--kernel` |
| `dd-rust-dd` | the real thing on the differential-dataflow crate | `--dd-rust-dd` errors "not built yet"; a reserved arm slot for a separate arc |

**Workload + engines (correctness).** `just dd-grade` sweeps every conformance
fixture that has both a dd_plan JSON and an oracle tick log, runs the chosen
arm under `/usr/bin/time -l`, and byte-diffs its stdout against the oracle.
Each arm has its own ratchet (`v6/dd-runner/graded.<arm>.tsv`) plus an 8 MB
peak-RSS ceiling. `dd-grade` is a green-all leg (32), not a perf bench.

**Where numbers bank.** `v6/dd-runner/graded.<arm>.tsv` (byte-clean ratchet)
and the `DD-GRADE` gate line. Measured this lane: sqlite arm 134 of 203
byte-clean, peak RSS 4 MB; rust arm 104 of 203 byte-clean, peak RSS 2 MB.

**Run command.** `cd v6 && just dd-grade` (sqlite arm, the default);
`DD_RUNNER_ARM=--dd-diet-rust-rust just dd-grade` (rust arm).

**Expected wall time.** ~a few minutes, the swipl emitter + oracle sweep over
203 fixtures dominating.

**History.** Renamed 2026-08-12 from `--sqlite` / `--kernel` because the binary
contains zero differential-dataflow and the old "dd" naming misled a
measurement into quoting a dd LIBRARY oracle row as though this compiler
emitted it. The throughput gap is structural, not a size gap: entering the
arms into the dl6-bench / dred / bench-cli harnesses needs a
`.dl6`-text-to-dd_plan emitter door (`compile.pl:328` emits `emit_ts` only;
the dd_plan JSON builder, `6_emit_dd_plan.pl:33`, takes a fixture term with
embedded initial + schedule), which is compiler-side surface in files this
lane is fenced out of. Priced in `v6/bench-cli/CONTRACT.md` section 6 and
listed under Open items below.

---

## dl6 emitted bench — the ratchet subject

**Purpose.** What does the emitted prolog→TypeScript+SQLite runtime actually
cost to build one closure from an empty head, per shape? This is the ratchet
subject, traced per-statement into the gap between the emitted runtime and the
in-RAM ceiling.

**Workload + engines.** The prolog compiler emits `reachability.dl6` →
`reachability.ts` (`bench.sh`), and `bench.ts` feeds grid / layered / chain at
10k rows. `dl6-bench` runs grid only; `dl6-bench-full` adds layered + chain
(~35s more, `justfile`).

**Where numbers bank.** `v6/labs/exec_shootout/dl6/FACTS.md` and
`FACTS.json` (json is gitignored; FACTS.md is tracked). Header states the env
knobs; temp-then-move so a crashed run cannot truncate the bank.

**Run command.** `cd v6 && just dl6-bench` or `just dl6-bench-full`

**Expected wall time.** ~164s for `dl6-bench-full` (grid + layered + chain at
10k) measured this lane; `dl6-bench` (grid only) is ~30s of that.

**History.** This is the bench that failure-modes entry 45 exists to guard.
A 534-file snake_case rename (`4a9b45f7`) silently renamed a runtime reader key
but missed the three lab drivers, which run under `--experimental-transform-types`
where no typechecker could see the dead literal key. Result: `grid_10000`
fixpoint 1182→5627ms (parent-vs-culprit rerun 4.28x), peak RSS 621→1364MB, and
the full bench aborted node's heap at ~10M rows while truncating FACTS.md
through the `>` redirect. Checksums stayed identical throughout, so no content
gate caught it — only the missing time/RSS comparator. Fix was snake_casing the
drivers + temp-then-move; the rail is the budgeted cell below
(`docs/failure-modes.md:1748-1777`).

---

## dl6 retraction ticks — in-place DRed vs refCount

**Purpose.** What does an incremental maintenance tick cost under in-place
DRed versus the refCount full re-derivation, at grid 45x45 (~1.07M rows)?
The incremental-tick regime is where the in-place DRed path wins if anywhere;
the single build tick is a wash by construction.

**Workload + engines.** `incbench.ts` builds the closure on a grid 45x45,
then times insert-one-edge, delete-one-edge (no rows lost), delete-a-structural-
edge (−44 rows), and an empty drain tick, in-place vs refCount.

**Where numbers bank.** `v6/labs/exec_shootout/dl6/FACTS.dredland.md`
(landing receipt for `IDredPlan`).

**Run command.** `cd v6 && just dl6-dred-bench`

**Expected wall time.** ~20s measured this lane (grid 45x45 bench + incbench).

**History.** This is the retraction saga's incremental-tick half. The refCount
side reconciles twice per tick, so every delete rows about double its insert
twin; the in-place side drops the tail's head-UPDATE/antijoin/bulk-insert.
Banked deltas: insert-one-edge 48x, delete-one-edge 70x, structural delete
47x, empty drain 1926x; and the single build tick within noise (+1.5..3.8%).
The full verdict on sqlite retraction mechanics — fk_cascade ceiling, support-
count vs fixpoint-recompute, cycle safety — is
`plans/2026-07-28-sqlite-retraction-verdict.md`, which re-proves Q3 domination
in the real database.

---

## store retraction rig — hermetic engines

**Purpose.** At the DAG-retract regime, how do the hermetic engines — the
oracle, the sqlite strategies, dd, and the v6 store — compare on retract wall
time, statements, and RSS? This is the disk-class middle tier.

**Workload + engines.** Eleven engine entries in `bench/run.sh` (sqlite-mem,
sqlite-disk, dd, dbsp, swi-incr, swipl-pure, swi-sqlite, swi-ts, swi-emit,
tsv2-gen, v1-gen) over a scale sweep expressed as `layers x width`. At this
base sha the `sqlite_reach` / `dd_reach` / `dbsp_reach` example binaries were
folded out at `a7d5ad36` and are SKIPped; see Open items. Full ladder:
SCALES `2x200 6x2000 8x20000 10x50000 14x80000` plus the default 9-cell TSV2
and V1 ladders.

**Where numbers bank.** `v6/sprefa-store/PERF-REPORT.md` (the tracked readout);
each run renders `bench/out/results.csv`, charts, and `bench/out/REPORT.md`.

**Hermetic protocol** (`PERF-REPORT.md:3`): every engine runs HERMETICALLY
(one process each), memcap gun = 12288 MB, setup untimed, the generated graph
dropped before the measured retract so no metric counts corpus residence.
`correct` = output hash equals the oracle's.

**Run command.** `cd v6 && just bench` (full ladder); perf-all drives it at
smallest scale (`SCALES="2x200" TSV2_SCALES="1x1000" V1_SCALES="1x1000"`).

**Expected wall time.** Small scale ~4s measured this lane (mostly the tsv2-gen
 s1/1000 oracle gate). Full ladder: measured well past 3 minutes this lane
 (still inside the swipl-engine segment at ~4 min when stopped), which is why
 perf-all uses the small scale.

**History.** The rig is the Z-set/IVM head-to-head feasibility lab; its
`engines/*.sh` emit a shared CSV protocol, gnuplot renders, awk writes prose.
PERF-REPORT's hermetic-protocol paragraph is the citation that keeps the
matrix comparable across runs and across Rust vs TypeScript rows (the tsv2
rows mark `sqlite_hw` N/A because `@libsql/client` exposes no highwater
binding).

---

## dred profile — single-retract flame

**Purpose.** For one cycle-safe retract, where does the time physically go —
per-phase wall, per-statement ms, EXPLAIN QUERY PLAN of the hot joins, block
I/O, SQLite C-heap high-water? Deep flame, not a sweep.

**Workload + engines.** `examples/profile_dred.rs` builds a multi-cyclic graph
(default 6 layers x 160k width), EXPLAINs the two dominant joins, then measures
one `retract_dred`, bracketing I/O and heap.

**Where numbers bank.** Printed to stdout only; not banked to a file.

**Run command.** `cd v6/sprefa-store && cargo run --release --example profile_dred`

**Expected wall time.** ~5s measured this lane (default 6x160000 cyclic graph).

**History.** Backs the retraction verdict's claim that the production retract
is counting + two-pass over-delete/rederive over a SQL SCC nested fixpoint
(`v6/AGENTS.md` history, retraction ruling 2026-07-23). Single-retract flame
numbers for the cycle-safe path.

---

## dl6 budget cell — the regression gate

**Purpose.** Hold the emitted runtime to a ceiling so failure-modes entry 45's
class (a 4.3x time cliff with identical checksums) cannot ride green PRs again.
Ceilings ratchet DOWN only.

**Workload + engines.** Runs the grid bench, then grades `FACTS.json` against
`v6/labs/exec_shootout/dl6/budget.json`:
```json
{ "grid_10000": { "fixpoint_ms_ceiling": 2500, "peak_rss_mb_ceiling": 900 } }
```
Exit 2 on any breach.

**Where numbers bank.** `v6/labs/exec_shootout/dl6/budget.json` (the ceilings),
graded against the FACTS the bench just wrote.

**Run command.** `cd v6 && just dl6-budget`

**Expected wall time.** ~4s measured this lane (its internal bench is grid-only,
and it grades the FACTS file in place).

**History.** This is failure-modes rail-gap 45's missing rail, promoted into
the battery: the entry's RAIL line reads "missing — the promotion is a
budgeted bench cell in the battery (grid fixpoint time + RSS ceilings vs
banked FACTS.md)" (`docs/failure-modes.md:1827`). The budget cell closes that
gap. Its bench runs internally under its own run-capped budget
(`DL6_BUDGET_S`, default 300s, `budget-check.sh`).

---

## sqlite build baseline — landing

**Purpose.** A hand-tuned pure-sqlite closure build — the emitter's ratchet
target, the disk-class middle made honest. Until it lands, `dl6-bench-full`
is compared only against the in-RAM ceiling, with the sqlite middle inferred
from the store rig's sqlite strategies.

**Status.** LANDED. The `labs/exec_shootout/sqlite_baseline` binary exists on
disk (committed) and is wired into `just perf-all` (grid_10000, naive variant).
Its numbers are banked in `labs/exec_shootout/dl6/BASELINE.md`.

**Run command.** `cd v6/labs/exec_shootout/sqlite_baseline && cargo build --release && ./target/release/sqlite_baseline --case grid_10000 --variant naive --runs 3`

**Expected wall time.** single grid_10000 naive run ~1.5s (best of 3); the crate
build dominates, a few seconds warm.

---

## shared frontier arms — per_rel vs frontier(shared)

**Purpose.** Price the `frontier(shared)` compile option against the default
`per_rel` on the Rust door: what it costs in emitted text and what it buys per
tick. `plans/2026-08-19-shared-sqlite-frontier.md` justified the arc on codegen
size and table count; these are the first numbers taken through the shipped
compiler rather than a hand-written rig.

**Run commands.**

```
bash v6/sprefa-engine-rs/shared-frontier-bench.sh          # emitted bytes, statements/fold, fold ms
bash v6/sprefa-engine-rs/shared-frontier-grade.sh          # shared arm vs the oracle, whole corpus
bash v6/sprefa-engine-rs/shared-frontier-gate.sh           # per_rel vs shared tick logs, 8 fixtures
```

**Expected wall time.** bench ~40s, grade ~4 min, gate ~15s.

### Measured 2026-08-22 at `c88ebb0fd`, Apple M2 Pro

The wide fixtures are `v6/sprefa-engine-rs/tests/shared_frontier_wide/`: N
source rels, N derived rels behind a guard rule, 3 ticks, every source rel
touched every tick. `generate.py` regenerates them at any N.

| program | rels | emitted bytes per_rel | shared | statements/fold per_rel | shared | delta | fold ms per_rel | shared | delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| wide_4 | 8 | 39,272 | 43,178 | 367 | 290 | -21.0% | 32.2 | 31.9 | -0.9% |
| wide_16 | 32 | 155,456 | 169,024 | 1,447 | 1,082 | -25.2% | 85.0 | 67.0 | -21.2% |
| wide_64 | 128 | 621,200 | 673,636 | 5,767 | 4,250 | -26.3% | 255.7 | 230.8 | -9.7% |

**Read the statement count, not the wall.** Every cell above is one script run;
the statement count came back identical in all three runs of every cell and in
all three separate script invocations, and the emitted byte counts are exact.
The fold wall is the noisy column: across three invocations the shared arm was
faster in every cell of every one, by -0.9% to -6.1% on wide_4, -5.3% to -21.2%
on wide_16 and -9.7% to -14.0% on wide_64. Its first fold of a script run pays a
cold start (one wide_16 `per_rel` run read 1100.5 ms against 82.3 and 85.0 for
its siblings), which the median absorbs and a single reading would not.

Emitted bytes move +8.4% to +9.9% the wrong way, and that direction is stable.

### Boot DDL, every corpus fixture the shared guard admits

202 of the 341 fixtures that lower, both arms lowered in one process.

| metric | per_rel | shared | delta |
| --- | ---: | ---: | ---: |
| DDL statements | 6,737 | 7,056 | +4.7% |
| DDL bytes | 1,340,274 | 1,538,745 | +14.8% |
| ... of that, frontier objects | 397,463 | 595,934 | +49.9% |
| ... of that, every other statement | 942,811 | 942,811 | 0.0% |
| TEMP tables | 3,450 | 2,674 | -22.5% |
| indexes | 2,336 | 2,049 | -12.3% |
| TEMP views | 892 | 2,274 | +154.9% |

**The plan's codegen-size claim is inverted by the shipped lowering, and the
inversion is entirely inside the frontier objects.** Every non-frontier
statement is byte-identical between the arms. `lower.pl` keeps every per-rel
frontier NAME alive as a TEMP view over the shared pair so compiled reads keep
their text (`shared_frontier_view_ddl/3`), and a view carrying the payload
column list plus the join is longer than the `CREATE TEMP TABLE` it replaced.
Three objects per rel become two, so the object count falls; the text rises.

### Correctness of the shared arm, whole corpus

`shared-frontier-grade.sh` compiles every conformance fixture in shared mode and
diffs its Rust fold against the same oracle tick log `grade.sh` uses.

```
SHARED-GRADE graded=440 byte-clean=200
  unsupported 238
```

Zero `diff`, zero `runtime-error`. Every fixture the guard admits agrees with
the oracle byte for byte. The gap to `grade.sh`'s `byte-clean=335` is 136 guard
stops, 135 of which are byte-clean under `per_rel`.

### The reach ceiling

The 136 stops, by reason, from that same run:

| reason | fixtures |
| --- | ---: |
| `edge_rules` | 72 |
| `aggregate_head` | 44 |
| `recursion` | 6 |
| `host` | 5 |
| `non_set_rel` | 7 |
| `retention` | 2 |

`v6/dl/ghcache/ghcache.dl6` does not compile under `frontier(shared)` either. It
stops at `unsupported_construct(frontier_shared_todo(edge_rules))`, and that is
the first of five families rather than the only one: 157 rels, 220 rules,
reasons `aggregate_head`-11, `edge_rules`-1, `host`-8, `non_set_rel`-4,
`tick`-1.

Every one is a TODO site in `lower.pl` `shared_frontier_todo/3` rather than a
measured impossibility, and none has been probed since it was written. The one
structural constraint the code does show: `shared_frontier_view_ddl/3` joins
`__frontier."row_id"` to the durable table's `__id`, so a frontier row with no
live durable row (`departure`) and a rel whose storage carries no `__id`
(`non_set_rel`) each need an answer before their guard lifts.

The default cannot flip while the option reaches no program anyone runs.

---

## Open items

- **with-dbsp head-to-head resident-RAM rerun.** The rig's `dbsp` engine and
  the PERF-REPORT's resident-RAM table want a head-to-head against dd and the
  store under one RAM budget on the current base. Blocked at this sha because
  the `dbsp_reach` example requires the `with-dbsp` feature built explicitly on
  stable (`Cargo.toml:60`) and it is not part of `cargo build --examples`.
- **dl6-budget outer cap.** `perf-all` wraps dl6-budget in its own outer
  run-capped budget on top of budget-check.sh's internal `DL6_BUDGET_S` cap;
  the internal named line is the one that fires first, by design.
- **dd_plan arms have no throughput bench (the user's stated gap).** The two
  dd_plan arms are graded for correctness (`just dd-grade`) but timed in no
  perf battery. Entering them into `dl6-bench` / `dl6-dred-bench` /
  bench-cli requires a `.dl6`-text-to-dd_plan emitter door: `compile_dl6/3`
  (`v6/prolog/compile/compile.pl:328`) hardcodes `emit_ts`, and the dd_plan
  JSON builder (`v6/prolog/compile/6_emit_dd_plan.pl:33`) takes a conformance
  fixture TERM whose initial + schedule are embedded, so it cannot take the
  bench-cli / dl6-bench external schedule. Priced at
  `v6/bench-cli/CONTRACT.md` section 6; a `perf-all` leg is withheld until the
  emitter door lands, because a leg that only re-runs a correctness sweep
  would bank no throughput number.

Every number claimed above cites the file that banks it. The measured wall
times are from this lane's `just perf-all` run at `e926a196` on 2026-08-10;
the transcript headers and timings are in ATLAS_REPORT.md at the worktree
root.
