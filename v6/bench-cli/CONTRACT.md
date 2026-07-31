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

For every case there is exactly one **referee**, and its stdout is the
reference log every engine on that case is `cmp`-ed against byte for byte.
Which engine refereed is recorded per cell in the `referee` column and again in
the verdict spelling; a reader never has to infer it. Section 7 states when the
referee is allowed to be something other than swipl.

| verdict | referee | meaning |
|---|---|---|
| `identical` | `oracle` | byte-equal to the **swipl oracle**, the semantic authority |
| `identical_vs_reference` | `tsv2(proven)` | byte-equal to the **proven reference engine**, used only where swipl exceeded its budget and only under section 7's proof |
| `wrong` | either | ran, produced a log, log or final-state hash differs from the referee's |
| `refused` | either | exit 2, named construct unsupported |
| `error` | either | exit anything else |
| `over_budget` | `none` | this is the ORACLE's own row on a case where it exceeded the reference budget |
| `no_reference` | `none` | nothing graded this cell; the reason names whose ceiling it was |

**Only an engine that reproduced its referee is timed.** Both passing verdicts
are timed and they are not otherwise ranked differently; the distinction is
about the strength of the claim, not the quality of the number.

`no_reference` exists because the alternative is a lie. When the reference leg
fails and no proven referee is available, `cmp` against a missing or truncated
reference calls every other engine `wrong` — reporting an engine defect where
the real fact is that no referee reached that cell. Under `no_reference` the
engine is not timed and the reason line names whose ceiling it is.

**An engine without a matching log is never timed.** This is the v1 asymmetry
lesson made structural: `SCALE.md` recorded v1 as ~10x faster than tsv2 on
s2/100k while v1 emitted no delta log at all, so it was never paying the
tick-log obligation it was being compared on. Under this contract that run
does not produce a number.

### 2.6 Standings CSV

```
case,family,engine,verdict,referee,wall_ms,compile_ms,ticks,statements,peak_rss_mb,db_bytes,final_hash,input_hash,note
```

- `input_hash` = sha256 of `program bytes || 0x00 || schedule bytes`, first
  16 hex chars. **All engines on one case must show the same hash**, the
  PERF-REPORT "all engines must match" convention.
- `referee` = which engine graded this cell: `oracle`, `tsv2(proven)`, `none`.
- `wall_ms` is the engine-reported median over `BENCH_RUNS` runs (default 5).
- `peak_rss_mb` from `/usr/bin/time -l maximum resident set size`.
- `final_hash` = the third check, section 2.7.
- Any `N/A` cell has its reason in the standings' reason list.

The external-wall column is priced in section 1 and not currently emitted: with
hyperfine absent the repeat loop's `/usr/bin/time` median carries the ~30 ms
startup floor unequally across engines, which is the exact number this contract
refuses to rank on. It returns as a column the day hyperfine is installed.

### 2.7 Final-state hash — the third check

Every cell carries `final_hash` at every scale: the first 16 hex of the sha256
of one canonical final-state line, `{"final":{"<rel>":[[row],...]}}` with rel
names sorted, row texts sorted, empty rels dropped, and every value encoded by
the SAME `TickLogEmitter.valueText` the tick log uses.

This is the sweep's own final-state leg (`oracle_final/2` writes
`<name>.oracle.final.jsonl`; `v6/tsv2/scripts/sweep.ts` diffs it) brought into
the bench, and it is a check, not a decoration:

- On a **program case** the check is CROSS-ENGINE. The oracle's side is the
  committed `<name>.oracle.final.jsonl` artifact for the same fixture and the
  same schedule bytes; tsv2's side is computed live from the database at the
  end of the run. The two matching is a second, independent agreement beside
  the tick-log diff.
- On a **scale case** there is no fixture and so no swept oracle final state;
  the oracle row's `final_hash` is `N/A`. What the column checks there is
  cross-RUN: the reference invocation's hash against each of the timed repeats.
- Either way, a cell whose tick log matches and whose final-state hash does not
  is **`wrong`**. Sabotage receipt (c) in `bench.sh`'s header is exactly that
  case: tick logs agreed, one byte of the reference final-state line changed,
  cell flipped `wrong`, exit 1.

Why it earns its place: the tick-log diff is a statement about the *sequence*
of deltas, and the ruling keeps final state as a third check precisely because
a divergence that cancels out by the last tick and a divergence that never
shows in a delta are different failure shapes. It is also the only check that
says anything at all about an EMPTY-schedule case, which the tick-log diff
calls identical on no evidence (`SCOREBOARD.md` Finding 2's vacuous-pass class).

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

**Measured, because the reference tier leans harder on this than one `cmp`
supports.** Compiling every one of the 190 tick-log-identical fixtures through
the text door and `cmp`-ing against the sweep's own module: **96 byte-identical,
94 differ.** Every difference measured is the same one — `Initial` fixture
facts, which a `.dl6` text file has no spelling for, so they are present as
`boot` statements in the term-door module and absent from the text-door one:

```
$ diff out/mod_filter_map_is_a_level_rule.ts \
       ../prolog/compile/out/filter_map_is_a_level_rule.ts
118a119,120
>   { sql: `INSERT OR IGNORE INTO "reading" (...) VALUES (?, ?)`, params: ["cpu", 12] },
>   { sql: `INSERT OR IGNORE INTO "reading" (...) VALUES (?, ?)`, params: ["disk", 4] },
```

That is a limit of the .dl6 SURFACE, not a compiler divergence, and it is the
same one `schedule-gen.ts`'s header records for s2's link rows. **All nine
program cases in `cases.json` are in the byte-identical 96**, which is what the
claim above needs; a case that was not would be timing a program with a
different starting world than the sweep graded, and should be moved or seeded
through its schedule.

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
BENCH_CASES=match_classify just bench-cli            # one case (writes into out/)
BENCH_ORACLE_BUDGET=180 just bench-cli               # give swipl longer before deferring
```

`just sweep` is the upstream of the reference tier: it writes the artifacts
`proof.ts` reads. A tree whose sweep has never run grades everything swipl
reaches and refuses the rest, loudly (exit 1). Full run ~5 min on the machine
above, of which ~2.5 min is the five cases where swipl runs out its budget.

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
- **oracle-side `wall_ms`** — measured by `adapters/oracle.sh` around the whole
  swipl process, so it carries the ~10-20 ms startup floor while tsv2's number
  excludes node's. Making the two spans comparable means adding a timing goal
  inside `dl6_oracle.pl`, a file this lane is fenced out of. **Priced: one
  `statistics/2` pair around `print_ticklog/3` plus a perf-JSON write, small,
  but it is compiler-side surface and wants its own review.**

## 7. The referee at scale: the ruling, the promotion rule, the risk

Phase 0 recorded a finding (section 7.1 below, kept verbatim): the reference
engine that produces the left-hand side of every diff does not reach 10k rows,
so a rust engine could not be graded at competition scale. That is now ruled
and implemented.

### 7.0.1 The ruling

> `bench_reference = proven_engine_reference` — user 2026-07-31, `rulings.pl`.
> The big-scale referee is a pinned engine (tsv2 first) that EARNS reference
> status: byte-proven against the swipl oracle over the entire oracle-reachable
> corpus on every sweep; final-state hash retained as a third check at all
> scales. swipl stays the semantic authority where it reaches; rust is graded
> tick-log byte-diff against the proven reference beyond.

This is exit (1) of the two the phase-0 finding priced ("a reference that
scales"), and it keeps exit (2)'s final-state hash as a check rather than as a
substitute for the log diff.

### 7.0.2 The promotion rule, exactly as implemented

A cell is graded `identical_vs_reference` against the tsv2 adapter's log **only
when all three hold**. Any one missing and the cell is `no_reference` and the
run exits 1.

1. **The oracle exceeded its BUDGET on this cell** — exit 124 from the
   process-group timeout, and only 124. `BENCH_ORACLE_BUDGET` defaults to 30s;
   the slowest case swipl actually reaches is s2/1k at ~2.3s and the fastest it
   does not was still running past 183s, so every budget on that plateau defers
   the same five cells. Any OTHER non-zero oracle exit is `error` +
   `no_reference`: an unexplained reference failure is the moment when grading
   past the reference is least defensible, and a crash must not buy a promotion
   that a timeout buys.
2. **BREADTH: the sweep's artifacts record total oracle agreement.** `proof.ts`
   reads `v6/prolog/compile/out/manifest.json` and `out/run-results.json` —
   the files `just sweep` writes — and requires: zero `wrong`, zero
   `emitted_crash`, zero `no_oracle_log`, zero `final_wrong`, zero compiler
   crashes, `identical > 0`, `run-results` covering exactly the set the
   manifest calls compiled, and `identical + rejection` accounting for every
   run record. The artifacts are consumed, not recomputed: re-running 191
   fixtures inside a bench run is a second, slower copy of `just sweep`. The
   standings record the `sweep_sha` (sha256 over both artifacts, first 16 hex)
   and every bucket count, so the exact proof a table leaned on is nameable.
3. **CURRENCY: this run re-proves the referee where swipl reaches.** Artifacts
   are a claim about the tree at sweep time and cannot know whether the
   compiler or runtime moved since. So pass 1 grades every case swipl DOES
   reach against swipl, using the same adapter that would referee at scale, and
   a single failure there refuses the whole tier. Today that is 11 cells, nine
   of them real programs. Breadth from the artifacts, currency from the run;
   neither alone is the rule.

The reference log for a promoted cell is a **separate invocation** from the
timed repeats. With one engine in the field, diffing an engine against its own
single run would be vacuous; diffing five runs against a sixth is a real
repeatability check, and it is the exact seam a rust adapter plugs into with no
change to the harness.

**Gate.** `report.ts` exits 1 on: a promoted cell with no valid proof behind it;
any cell left ungraded because the proof is missing or invalid (the message
names the reason and says `run just sweep`); any cell that failed its referee;
input-hash disagreement. `bench.sh`'s floor gate — a run that timed nothing is
a broken rig, not a slow one — still runs after it, now reading the CSV that
run actually wrote.

### 7.0.3 The residual risk, stated rather than papered over

**A bug shared by tsv2 and a future rust engine passes the tick-log diff at big
scale.** The proven referee is proven against swipl only where swipl reaches;
beyond that budget, two engines that are wrong in the same way agree with each
other and the diff is silent. Nothing in this tier can close that, because
there is no third opinion out there to ask.

The two mitigations are the ones the ruling names, and they are mitigations,
not a fix:

- **the small-scale swipl proof**, which is where the semantics actually live.
  A shared bug has to be scale-dependent to survive it: it must not fire on any
  of the 190 corpus fixtures nor on any of the 11 cells swipl referees in the
  run itself. That is a narrow shape, and it is exactly the shape (arithmetic
  that overflows only past some row count, an index the planner abandons at
  size, a cache that only fills at scale) worth naming as the thing this bench
  cannot see.
- **the final-state hash**, which is a second reading of the same run taken
  through a different path — a SELECT over the final tables rather than the
  per-tick delta stream. It catches a divergence the delta log cancels out; it
  does not catch a divergence both readings share.

The honest summary: at s1/1k the standings assert equality with the language's
specification. At s1/100k they assert equality with an engine that was equal to
the specification everywhere the specification could be run. Those are different
claims, and the `identical` / `identical_vs_reference` split exists so that no
reader has to remember which is which.

## 7.1 THE PHASE-0 FINDING (kept verbatim; the ruling above is its answer)

The result phase 1 most needs, and it is about the ORACLE rather than tsv2.

The tick-log byte-diff is what makes this contract honest, and **the reference
engine that produces the left-hand side of that diff does not reach 10k rows.**
Measured:

| cell | oracle | tsv2 | verdict |
|---|---:|---:|---|
| s1/1k | 1325 ms | 33.1 ms | identical |
| s2/1k | 2230 ms | 24.0 ms | identical |
| s1/10k | > 180 s (173.58 s killed, then still running past 183 s) | — | `no_reference` |
| s2/10k | walls | — | `no_reference` |
| s3/1k | walls | — | `no_reference` |

The two engines are already ~40-90x apart at 1k, and the gap grows in the
direction that removes the referee. PERF-REPORT competes at 960k. So a rust
engine cannot be graded at competition scale by this method as it stands.

Two ways out, neither taken here because both are design decisions rather than
harness work:

1. **A reference that scales.** The oracle is `engine.pl` interpreting the
   program; it is the spec, and its speed was never the point.
2. **A cheaper invariant at scale.** The full per-tick log is the strongest
   check and the most expensive. The sweep already computes a FINAL-STATE line
   (`oracle_final/2`, `<name>.oracle.final.jsonl`); a final-state hash would
   grade s1/10k for a fraction of the cost, at the price of not catching a
   divergence that cancels out by the last tick. Tiered grading — full tick
   log where the oracle reaches, final-state hash beyond — is the obvious
   shape, and it is a ruling, not a refactor.

Recorded rather than worked around: a bench that quietly stopped grading at
scale would be exactly the v1 asymmetry wearing a different hat.

**What the ruling changed, cell by cell.** The three `no_reference` rows above
are graded now, and two cells the finding could not even list were added
because the referee reaches them:

| cell | phase 0 | now | tsv2 wall ms |
|---|---|---|---:|
| s1/10k | `no_reference` | `identical_vs_reference` | 280.0 |
| s1/100k | not benched | `identical_vs_reference` | 2610.4 |
| s2/10k | `no_reference` | `identical_vs_reference` | 153.9 |
| s2/100k | not benched | `identical_vs_reference` | 1420.7 |
| s3/1k | `no_reference` (and `SCALE.md` DNF) | `identical_vs_reference` | 9928.2 |

s3 stays at 1k: its shape is a 2-atom cross join, quadratic on purpose, and
1000x1000 already produces the 1M combined rows the memory column is there to
watch (929.7 MB peak RSS against a 512 MB V8 old-space cap — most of that is
sqlite, outside the heap the flag bounds).
