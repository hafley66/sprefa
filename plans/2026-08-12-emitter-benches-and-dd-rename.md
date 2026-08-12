# REPORT — emitter benches + dd-runner rename

Lane: `fix/emitter-benches-and-dd-rename`. Started on `9ecd3341`
(`git merge --ff-only` reported "Already up to date"); shipped two commits on
HEAD `e7558fc9`:

| commit | what |
|---|---|
| `a88a02aa` | dd-runner rename: arms say what they are, per-arm grade ratchet |
| `010c40bb` | bench-cli: repair tsv2_run.ts snake_case drift; add dd_plan arm adapters |

## 1. Which emitter output appears in which harness

| emitter output | correctness | throughput (timed) | harness |
|---|---|---|---|
| tsv2 (`emit_ts.pl`) | sweep 283, conformance 281 | YES | bench-cli, dl6-bench, dl6-budget, dl6-dred-bench, shootout, store rig |
| dd_plan -> `dd-diet-rust-sqlite` (was `--sqlite`) | dd-grade 134/203 byte-clean | NONE | dd-grade only (correctness + peak RSS, a green leg, not a perf bench) |
| dd_plan -> `dd-diet-rust-rust` (was `--kernel`) | dd-grade 104/203 byte-clean | NONE | dd-grade only |
| dd_plan -> `dd-rust-dd` (differential-dataflow crate) | not built | NONE | reserved arm slot, errors "not built yet" |

The user's stated gap (both dd_plan arms never timed against anything) is
confirmed and named as an open item in `v6/labs/BENCHMARKS.md`. The arms are
graded for correctness but enter no timing harness.

## 2. Arms that could not satisfy a contract

Both dd_plan arms fail the bench-cli contract, and the adapters record it as a
named refusal (exit 2) rather than faking a row.

| arm | failing clause | citation | status |
|---|---|---|---|
| `dd-diet-rust-sqlite` | contract clause **2.1**: `--program <file.dl6>` is a `.dl6` text and the schedule is external; the arm takes a `dd_plan` JSON whose initial + schedule the emitter embeds | `v6/bench-cli/CONTRACT.md:199-209`; `v6/prolog/compile/6_emit_dd_plan.pl:33`; text door `compile_dl6/3` emits `emit_ts` only, `v6/prolog/compile/compile.pl:328` | adapter `adapters/dd-diet-rust-sqlite.sh`, refuses every case (exit 2), priced in CONTRACT.md section 6 |
| `dd-diet-rust-rust` | contract clause **2.1**, same as above; additionally no `<perf-out>.final.jsonl` writer (third check, CONTRACT.md section 2.7) | as above | adapter `adapters/dd-diet-rust-rust.sh`, refuses every case (exit 2), priced in CONTRACT.md section 6 |

There is no `.dl6`-text-to-dd_plan emitter door in the repo; wiring one routes
through `compile_program_phases/8` (compile.pl:346) plus a final-state writer,
both compiler-side files fenced to other lanes. Priced at CONTRACT.md section 6.

## 3. bench-cli expected line, before and after

Not changed. The dd_plan adapters refuse and are not wired into the harness
(`bench.sh`'s `engine_cmd`, `bench.sh:155-161`, is outside this lane's file
list), so they add no cells. The expected line at `v6/justfile:407-412` stays:

```
BENCH-CLI timed=16 (swipl 11 / reference 5) disqualified=0 ungraded=0 hash-agreement=OK, exit 0
```

This lane's full run did NOT reach that line, and the cause below is outside
the lane's files.

## 4. Gate commands with pasted output

```
$ just --justfile v6/justfile dd-grade
DD-GRADE arm=--dd-diet-rust-sqlite graded=203 byte-clean=134 peak_rss_mb=4 (4576 kB, clean_state_gate_and_exit_zero) ceiling=8
DD-GRADE HOLDS
```

```
$ DD_RUNNER_ARM=--dd-diet-rust-rust DD_RUNNER_WRITE_GRADED=1 bash v6/dd-runner/grade.sh
DD-GRADE arm=--dd-diet-rust-rust graded=203 byte-clean=104 peak_rss_mb=2 (2608 kB, fix_by_waiver_returns_to_clean) ceiling=8
(graded.dd-diet-rust-rust.tsv written on this run; the next run is the ratchet verify)
```

```
$ just --justfile v6/justfile dl6-bench
| `grid_10000` | 3,960 | 1,069,200 | `9d7239568960d6a8` | 34 | 1312 | 814,939 | 596 MB |
(rc=0)
```

```
$ cd v6/prolog && swipl -g go -t halt ARCH.pl
PASS  construct_status_closed
PASS  construct_tier_known
PASS  covers_endpoints_ground
(rc=0)
```

```
$ just --justfile v6/justfile bench-cli
BENCH-CLI timed=8 (swipl 8 / reference 0) disqualified=3 ungraded=5 hash-agreement=OK
BENCH-CLI GATE: tsv2 on clock_rel_join_storms did not reproduce its referee (oracle): tick log differs from the referee's
(rc=1)
```

`dd-grade`, `dl6-bench`, `ARCH.pl` are GREEN. `bench-cli` is RED, explained
below.

### The bench-cli red

At HEAD, before this lane, `bench-cli` was already red: the tsv2 adapter
crashed on a snake_case drift and every tsv2 cell errored
(`timed=0 disqualified=11`). The lane repaired it (commit `010c40bb`): the
runtime renamed `rowValueFromSql`/`finalSelect`/`relColumns`/`relColumnTypes`/
`valueText` to snake_case, and `adapters/tsv2_run.ts` still bound the old
names. Fixed -> 8 of 11 program cases back to `identical`.

The remaining 3 cells + their 5 downstream `no_reference` rows are drift owned
by other files, cited:

| case | verdict | cause | file not in this lane |
|---|---|---|---|
| `clock_rel_join_storms` | wrong | runtime tick log string-encodes integer columns (`"3"` vs oracle `3`); the canonical ruling demands integers as JSON numbers | `v6/tsv2/runtime/` |
| `diag_seven_ticks` | refused | compiler refuses rule `comparison_operand_not_number` | `v6/prolog/compile/`, concurrent `fix-zero-column-ref-target` lane owns `lower.pl` / `0_generic_expand.pl` |
| `aggregate_retraction` | refused | compiler refuses rule `aggregate_operand_not_number` | same |

The 5 scale cells graded `no_reference` because the run's currency gate gives
up when 3 of 11 live cells fail (`,bench.sh:475-482`). These compiler/runtime
regressions belong to the concurrent compiler lane and the tsv2 runtime, not
this lane.

## 5. Checksum agreement per case per arm

dl6-bench (emitted TS runtime), the only harness the lane could run:

| case | derived | checksum | expected | agreement |
|---|---|---|---|---|
| grid_10000 | 1,069,200 | `9d7239568960d6a8` | `9d7239568960d6a8` | MATCH |

The two dd_plan arms could not be run on these cases: entering them needs the
text-door dd_plan emitter that does not exist (section 2), and per the lane
rule an arm that cannot produce the bench's checksum on the same input is not
staged.

## 6. What I did NOT do, and why

- Did not build `dd-rust-dd` (the differential-dataflow crate arm). It is the
  priced kernel of `plans/2026-08-10-dd-dance-recon.PLAN.md:136-141`, a
  separate arc. Left the `--dd-rust-dd` slot reserving it, erroring "not built
  yet".
- Did not enter the dd_plan arms into `dl6-bench` / `dl6-dred-bench` /
  `bench-cli`. The blocking clause is the missing `.dl6`-text-to-dd_plan
  emitter door (section 2). Adding it edits `compile.pl` / `6_emit_dd_plan.pl`,
  fenced to other lanes. The lane's own rule: "if a bench needs an emitter
  change, STOP AND REPORT."
- Did not add a `perf-all` leg for the arms. A leg that only re-runs a
  correctness sweep would bank no throughput number, and the arms run under no
  timing harness yet. Withheld with reason in `v6/labs/BENCHMARKS.md` open
  items, to be added the day the emitter door lands.
- Did not write a new bench script (the user's explicit point).
- Did not open a PR.
- Did not edit `lower.pl`, `0_generic_expand.pl`, or `6_emit_dd_plan.pl`
  (fenced). Did not edit dated `plans/` / `chat_log/` / `DD_RUNNER_B1_REPORT.md`
  records that still spell the old arm names; they are historical records. The
  `--sqlite` hits in `symmetries.html` and the v4 chat logs are an unrelated
  v4/SQLite and a CSS token, not this binary.

## Rename arbitration

Files carrying the arm names were updated: `v6/dd-runner/src/main.rs` (flags +
usage + `--dd-rust-dd` refusal), `v6/dd-runner/grade.sh` (per-arm ratchet,
default `--dd-diet-rust-sqlite`), `graded.tsv` -> `graded.dd-diet-rust-sqlite.tsv`
(+ new `graded.dd-diet-rust-rust.tsv`), `v6/justfile` (dd-grade expect line,
split `green` unchanged), `v6/prolog/ARCH.pl` (task-row prose),
`v6/labs/BENCHMARKS.md` (arms + gap).
