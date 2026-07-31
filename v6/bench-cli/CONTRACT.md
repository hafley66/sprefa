# Language-agnostic CLI bench contract

Phase 0 of `plans/2026-07-31-rust-course.md`. This is the gate that precedes
any rust lowering: one CLI shape any engine implementation can satisfy, a
correctness referee that disqualifies an engine which cannot produce the log,
and a standings table whose numbers are comparable across languages.

Brief: `plans/2026-07-31-bench-cli-brief.md`.

Machine for every number below: Apple M2 Pro, 16 GB, macOS 23.6.0,
Node v24.15.0, SWI-Prolog 10.0.2 arm64-darwin.

---

## 1. Build-vs-buy: the timing leg

Standing law: no bespoke harness line before a written candidate analysis.
Here it is, with the measurements that decided it.

### What has to be measured

Three different quantities, and they do not share a tool:

| quantity | why it is in the standings |
|---|---|
| **run wall time** | the number the rust-vs-TS decision turns on |
| **peak RSS** | the s3 OOM and the 682 MB tsv2 row in `SCALE.md` are memory findings, not time findings |
| **statements / ticks / db bytes** | engine-internal; no external tool can see them |

### Candidate 1 — hyperfine 1.20.0

The obvious buy, and the brief expected it to win. Status on this machine:

```
$ command -v hyperfine        # (no output)
$ brew info hyperfine
==> hyperfine: stable 1.20.0 (bottled), HEAD
Not installed
```

Bottled and one `brew install` away, but **not installed**, and the dispatch
fence forbids a system-wide install without receipts. That is the smaller
objection. The larger one is structural, and it would apply even if hyperfine
were already on the box:

- hyperfine times **process wall clock**. Every engine in this contract pays a
  different fixed interpreter-startup cost before it executes one tick:

  ```
  interpreter startup floor, 3 runs each (/usr/bin/time -p, seconds)
    node -e '0'                 0.04  0.05  0.03
    swipl -g halt -t halt       0.02  0.01  0.02
  ```

  30-50 ms for node against 10-20 ms for swipl. A ~30 ms constant that has
  nothing to do with engine speed, charged unequally, against fixture cells
  whose real engine work is single-digit milliseconds (`SCALE.md` s2/1k =
  47.5 ms total wall for tsv2, 4.8 ms for v1). Ranking engines on that number
  ranks their runtimes' boot code.
- hyperfine cannot average a value the child process writes to a file. The
  primary metric here is engine-reported, so hyperfine's statistics machinery
  (warmups, outlier detection, mean±σ) has nothing to chew on for the column
  that matters.
- hyperfine reports no peak RSS.

**Verdict: BUY, but for the secondary column only, and optionally.** The
harness runs under hyperfine when `hyperfine` is on PATH (`--export-json`
parsed for the external-wall column), and falls back to a repeat loop when it
is not. Nothing about the standings changes either way, because hyperfine
never feeds the primary number. Installing it is a strict upgrade to one
column and is the recommended follow-up — it is not a blocker, which is why
this lane did not install it.

### Candidate 2 — `/usr/bin/time -l` (macOS BSD time)

Already installed, and already **this repo's own RSS instrument**:
`v6/sprefa-store/PERF-REPORT.md` states "`rss` = process resident high-water
from `/usr/bin/time -l`", and `SCALE.md`'s DNF records quote it directly.

Resolution receipt:

```
$ /usr/bin/time -p sleep 0.123
real 0.13

$ for i in 1..5; /usr/bin/time -l node -e '0'   (real column)
0.05  0.03  0.07  0.06  0.03
```

10 ms quantisation on `real`, and a 2.3x spread on an empty node process. Not
usable as the primary wall clock; **exactly right** for
`maximum resident set size`, which it reports in bytes and which nothing else
available reports at all.

**Verdict: BUY for the peak-RSS column.** House convention already, zero new
dependency, and it keeps the bench-cli standings mergeable with PERF-REPORT's.

### Candidate 3 — engine-internal wall clock (`--perf-out`)

Each adapter times its own run phase (`performance.now()` in node,
`statistics/2` or wall time in swipl) and writes it to a JSON file.

- Sub-millisecond resolution.
- Excludes interpreter startup, program compile, and schedule parse — the
  three costs that are not the engine.
- Every implementation language can do this; it is a JSON file, not an API.
- Costs one contract field per engine, which is the thing being designed
  anyway.

Precedent in-tree: `SCALE.md` already reports `mean tick ms` / `p95 tick ms`
from in-process instrumentation, not from an external timer.

**Verdict: BUILD, ~20 lines per adapter, and it is the primary number.** No
off-the-shelf tool can produce it, because it is by definition inside the
process being measured.

### Candidate 4 — criterion / divan / bencher (Rust in-process)

Real harnesses with real statistics, and `perf:biome-benchmarks` /
`perf:oxc-benchmarks` document them as the correct pick for Rust
microbenchmarks. Wrong shape here on one hard point: they are **in-process
Rust libraries**. They cannot time a swipl process or a node process, which
are two of the three engines at contract birth. Adopting one would mean the
rust engine is measured by a different instrument than its competitors, which
is the exact asymmetry the correctness leg exists to prevent.

`bencher` (bencher.dev) additionally is a hosted CI-tracking service; the
tracking problem is not the phase-0 problem.

**Verdict: REJECT for the cross-language contract.** Correct tool for a
future rust-internal microbench once phase 1 picks a rust shape; it does not
grade engines against each other.

### Candidate 5 — tinybench / mitata (JS in-process)

Same objection, mirrored: JS-only, so they measure the TS engine with an
instrument the oracle and any rust engine cannot share. Also not present in
`v6/tsv2/node_modules` (checked), so adopting one is a new dependency for a
number `performance.now()` already gives.

**Verdict: REJECT.**

### Candidate 6 — the in-tree engines rig, `v6/sprefa-store/bench/run.sh`

The closest thing to an existing answer, and it is reused — partially.

What it is: a scale-sweep driver over compiled Rust examples plus shell
wrappers, each printing one `CSV,...` line on stderr, collected into
`engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb,host_peak_mb,sqlite_hw_mb,db_mb`,
then charted by gnuplot and written up by awk.

What does not fit: its unit of work is `layers x width` on one hardcoded
reachability shape, not `(program, schedule)`. It has **no correctness
referee** — `run.sh` greps for `TSV2_ORACLE_DIFF` and trusts each engine to
self-report. PERF-REPORT's `correct` column is computed by a separate leg.

**Verdict: ADOPT THE CONVENTIONS, NOT THE DRIVER.** The standings CSV extends
its column vocabulary (`host_peak_mb`, `db_mb`, N/A-with-reason, per-cell
input hash "all engines must match") so the two tables stay readable side by
side. The driver stays untouched (it is fenced read-only anyway).

### Candidate 7 — the correctness referee

Surveyed for completeness: nothing off the shelf diffs a tick-log JSONL
against a reference engine's tick-log JSONL under this repo's canonical-JSON
encoding ruling. `cmp` / `diff` do the byte comparison, and they are what the
harness shells out to. The *grading* around them (which cases are eligible,
what disqualification means, how a refusal differs from a wrong answer) is
project semantics.

**Verdict: BUILD, reusing `cmp`.** This matches the brief ("The correctness
referee (tick-log byte diff) is ours either way").

### Summary table

| leg | verdict | tool | why |
|---|---|---|---|
| run wall time (primary) | BUILD | adapter `--perf-out` | startup floor differs 30 ms across engines; no external tool sees inside |
| external wall (secondary) | BUY, optional | hyperfine if on PATH, else repeat loop | right tool, wrong metric to lead with; not installed |
| peak RSS | BUY | `/usr/bin/time -l` | already the house instrument (PERF-REPORT) |
| statements / ticks / db bytes | BUILD | adapter `--perf-out` | engine-internal by definition |
| CSV/table conventions | ADOPT | `bench/run.sh` + PERF-REPORT | keeps the two standings mergeable |
| correctness referee | BUILD | `cmp` + project grading | nothing off the shelf grades tick logs |

Priced and **not** taken this lane: installing hyperfine (one column upgrade,
needs a system-wide install the fence forbids); gnuplot charts (the existing
`chart.sh` covers the scale-sweep shape and this table is small enough to read).

---

## 2. The contract

### 2.1 An engine is one executable

```
<adapter> --program <file.dl6> --schedule <schedule.json> --db <path> --perf-out <file.json>
```

| flag | meaning |
|---|---|
| `--program` | a `.dl6` **text** program. Text, not a fixture term, so every engine reads the same bytes. |
| `--schedule` | arrival schedule JSON (§2.3). |
| `--db` | database path. `:memory:` is legal. An engine with no database ignores it and says so in `notes`. |
| `--perf-out` | where to write the perf JSON (§2.4). |

**stdout is the tick log and nothing else.** One JSONL line per tick, in tick
order, no trailing blank line beyond the final newline. Diagnostics,
warnings, and progress go to stderr. An adapter that prints anything else on
stdout is not conformant, because stdout is what gets byte-diffed.

**Exit codes.**

| code | meaning | harness treatment |
|---|---|---|
| 0 | ran to completion, tick log on stdout | graded |
| 2 | engine **refuses** this program (named unsupported construct) | `REFUSED`, not a failure; recorded with the reason |
| other | engine failed | `ERROR`, disqualified from timing |

The 0/2 split matters because the tsv2 compiler refuses by design (named
refusals are a defended property), and a refusal must not read as a bug.

### 2.2 Tick-log format

The item-9 cross-target log, unchanged, produced by
`v6/prolog/conformance/ticklog.pl` on the oracle side and
`v6/tsv2/runtime/ticklog.ts` on the tsv2 side. One tick per line:

```json
{"tick":1,"deltas":{"rel_name":{"add":[[...row...]],"del":[]}}}
```

Values follow the `json_ticklog_encoding = canonical_json_text` ruling:
integers as JSON numbers, json values as canonical JSON text (sorted keys, no
whitespace), everything else as a JSON string. This contract does not restate
the encoding; it points at the ruling, and any drift is a bug in the engine,
not in the bench.

### 2.3 Schedule format

The same file the http client posts and `sweep.pl` already writes: an array of
ticks, each an array of arrival rows.

```json
[[{"rel":"resp_raw","sign":"add","row":["api",200,"etag_1","payload_1"]}]]
```

`sign` is `"add"` or `"del"`. A `json`-typed column carries **JSON text**, not a
raw object (`dl6_oracle.pl`'s header states this and both doors agree).

### 2.4 Perf JSON (`--perf-out`)

```json
{
  "engine": "tsv2",
  "wall_ms": 12.437,
  "compile_ms": 431.2,
  "ticks": 1,
  "statements": 23,
  "db_bytes": 32768,
  "notes": {}
}
```

| field | required | N/A rule |
|---|---|---|
| `engine` | yes | — |
| `wall_ms` | yes | run phase only: after program load and schedule parse, through the last tick. Excludes process start and compile. |
| `compile_ms` | yes | `"N/A"` for an engine that interprets the source |
| `ticks` | yes | — |
| `statements` | yes | `"N/A"` for an engine that issues no SQL |
| `db_bytes` | yes | `"N/A"` for an engine with no on-disk database |
| `notes` | yes | free-form; **every `"N/A"` must have a matching reason string here.** |

The N/A-with-reason rule is inherited from PERF-REPORT, where
`sqlite_hw_mb = N/A` carries the reason "`@libsql/client` exposes neither a
`sqlite3_memory_highwater` binding nor an allocator-status API". A bare `N/A`
is not conformant.

### 2.5 Correctness referee

For every case, the **oracle adapter's stdout is the reference**. Every other
engine's stdout is `cmp`-ed against it byte for byte.

| verdict | meaning |
|---|---|
| `identical` | byte-equal to the oracle. **Only an `identical` engine is timed.** |
| `wrong` | ran, produced a log, log differs |
| `refused` | exit 2, named construct unsupported |
| `error` | exit anything else |
| `no_reference` | the ORACLE produced no log for this case, so nothing here can be graded |

`no_reference` exists because the alternative is a lie. When the reference leg
fails or times out, `cmp` against a missing or truncated reference calls every
other engine `wrong` — reporting an engine defect where the real fact is that
the reference engine did not reach that cell. Under `no_reference` the engine
is not run at all and the reason line names whose ceiling it is.

**An engine without a matching log is never timed.** This is the v1 asymmetry
lesson made structural: `SCALE.md` recorded v1 as ~10x faster than tsv2 on
s2/100k while v1 emitted no delta log at all, so it was never paying the
tick-log obligation it was being compared on. Under this contract that run
does not produce a number.

### 2.6 Standings CSV

```
case,engine,verdict,wall_ms,external_wall_ms,compile_ms,ticks,statements,peak_rss_mb,db_bytes,input_hash,notes
```

- `input_hash` = sha256 of `program bytes || 0x00 || schedule bytes`, first
  16 hex chars. **All engines on one case must show the same hash**, the
  PERF-REPORT "all engines must match" convention.
- `wall_ms` is the engine-reported median over `BENCH_RUNS` runs (default 5).
- `external_wall_ms` is hyperfine's mean when hyperfine is present, otherwise
  the repeat loop's median of `/usr/bin/time` wall. Carries the startup floor;
  read it as a sanity check, never as the ranking.
- `peak_rss_mb` from `/usr/bin/time -l maximum resident set size`.
- Any `N/A` cell has its reason in `notes`.

---

## 3. Adapters at contract birth

| engine | adapter | status |
|---|---|---|
| oracle (swipl reference engine) | `adapters/oracle.sh` | reference; produces the log every other engine is graded against |
| tsv2 (prolog -> TS, compiled) | `adapters/tsv2.sh` + `adapters/tsv2_run.ts` | full |
| v5 rust (`dl`) | not written — **priced in §6** | skipped with reason |

### How the tsv2 adapter avoids touching fenced code

An emitted module imports `../runtime/...` relative to itself, so it can only
run from a directory whose sibling is the tsv2 runtime. Rather than rewriting
the emitted bytes (they are a graded artifact elsewhere — byte-identity
against the oracle emitter is a standing receipt) or writing scratch modules
into `v6/tsv2/`, `v6/bench-cli/runtime` is a **committed relative symlink** to
`../tsv2/runtime`, and `v6/bench-cli/out/` holds compiled modules. The
emitted file is byte-for-byte what the compiler wrote, and no fenced file is
touched.

`v6/bench-cli/node_modules` is the same trick for dependency resolution and is
gitignored.

**Receipt that this bench times the graded engine.** The adapter compiles
`dl_view/<name>.dl6` through the TEXT door; the sweep compiles the fixture
TERM. The module that comes out is byte-identical:

```
$ cmp v6/bench-cli/out/match_classify_response.ts \
      v6/prolog/compile/out/match_classify_response.ts
$ echo $?
0
```

So the thing being timed here is the same module `just sweep` grades, not a
variant compiled down a bench-only path. (That the two doors agree is itself
the standing `TEXT_DOOR` receipt; this is that property being leaned on
rather than re-proven.)

---

## 4. Cases

Two families, both listed in `cases.json`.

**Fixture cases** — `.dl6` text from `v6/prolog/compile/dl_view/` with the
matching schedule from `v6/prolog/compile/out/`. These exercise real language
constructs (match blocks, aggregates, retraction, enum expansion, edge
carries) and are the correctness backbone.

**Scale cases** — generated programs at parameterised row counts, the s1/s2/s3
shapes `SCALE.md` already uses. See §6 for what was and was not taken.

---

## 5. Running it

```
cd v6 && just bench-cli              # full run, writes STANDINGS.md + standings.csv
BENCH_RUNS=9 just bench-cli          # more repeats
BENCH_CASES=match_classify_response just bench-cli   # one case
```

---

## 6. Priced and not taken

Recorded here rather than silently skipped.

- **hyperfine install** — §1 candidate 1. Upgrades `external_wall_ms` from a
  median-of-N loop to warmup + outlier-detected mean. One `brew install`; the
  harness already detects and prefers it. Not taken: system-wide install,
  fenced.
- **v5 rust adapter** — `dl` speaks `.dl` (v5 surface), not `.dl6`, and emits
  no tick log at all; it has no `--schedule` concept because it is
  file-watch-driven. Making it contract-conformant means a new v5 CLI mode
  that replays an arrival schedule and prints per-tick deltas — that is engine
  work inside `src/`, squarely outside this lane's fence. The flagship rig
  (`v6/tsv2/scripts/flagship-callgraph.sh`) already grades v5 against v6 by a
  different route (same program, same corpus, diff the relation contents), and
  that is the right precedent to extend when a v5 row is wanted.
  **Priced: a `dl bench --schedule` mode in `src/cli/`, plus a tick-log
  emitter over the v5 fixpoint. Not small, not this lane.**
- **s3 scale case** — the 2-atom combine cross join. `SCALE.md` records tsv2
  DNF at every s3 size (V8 heap ceiling) against v1 completing s3/1k. Included
  in `cases.json` as a **memory-wall** case, not a timing case: it is expected
  to produce `error`, and the standings record the wall rather than a number.
- **gnuplot charts** — `bench/chart.sh` exists and works for the scale-sweep
  shape. This table is small enough to read as markdown; wiring charts is
  cheap later.
