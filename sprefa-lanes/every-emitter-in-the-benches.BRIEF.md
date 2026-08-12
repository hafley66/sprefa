# LANE: every prolog emitter output enters the benches that already exist

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT. No archive/tar/copy workaround.

## WORKTREE SETUP BEFORE FIRST COMMIT (never `--no-verify`)
1. copy `v6/sprefa-extract/target/release/extract` from the main tree in
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## THE USER'S ASK, VERBATIM
"i want every emitter output of prolog to be tested yes so get A dispatched but
also...try to have it solve or interface with other benches spread throughout.
read justfile to find out where some tests are i've been trying to beat ai to
write things in there man, we have a fuck load of tests over time for many
things and subproblems and scalings"

READ THAT TWICE. The user has built many benches over a long time. YOU ARE NOT
ADDING A NEW BENCH. You are entering engines into harnesses that already exist
and already define their protocols. A new bench script is a FAILED lane.

## READ THESE BEFORE TYPING (they are the map)
| file | what it is |
|---|---|
| `v6/labs/BENCHMARKS.md` | the atlas, 297 lines, every bench with purpose + history + budget. Its "truth stack" at :27-44 ranks the layers |
| `v6/justfile:357-465` | every bench recipe: shootout, dl6-bench, dl6-dred-bench, dl6-budget, bench, bench-cli, perf-report, perf-all |
| `v6/bench-cli/CONTRACT.md` | the CLI shape ANY engine must satisfy. This is the phase-0 gate for rust |
| `v6/bench-cli/adapters/` | today: `oracle.sh`, `tsv2.sh`. That is all |
| `v6/sprefa-store/PERF-REPORT.md:5` | the four memory columns and what each means |

## THE MEASURED GAP YOU ARE CLOSING
The prolog compiler has three emitter outputs. Coverage today:

| emitter output | correctness | throughput |
|---|---|---|
| tsv2 (`emit_ts.pl`) | sweep 283, conformance 281 | YES: dl6-bench, dl6-budget, bench-cli |
| dd_plan -> `dd-runner --sqlite` | dd-grade 134 clean | NONE |
| dd_plan -> `dd-runner --kernel` | dd-grade, errors 33->7 | NONE |

Both dd_plan arms have NEVER been timed against anything. `v6/dd-runner` has
`--sqlite` (default) and `--kernel` arms, dispatched at `src/main.rs:107-108`.

## JOB 0: THE RENAME, DO THIS FIRST
The user's words: "we should try to make an actual dd runner, we can have
dd-diet-rust-sqlite and dd-diet-rust-rust and dd-rust-dd".

`dd-runner` contains ZERO differential dataflow. Its deps are `rusqlite`,
`serde`, `serde_json`; `kernel.rs:15` is a plain `BTreeMap<String,
Vec<Tuple>>`. The name has already caused a measurement error this session: a
`dd` row quoted from `PERF-REPORT.md` is the differential-dataflow LIBRARY
oracle in `sprefa-store`, not anything this compiler emits.

Rename the two existing arms to say what they are:

| today | becomes | what it is |
|---|---|---|
| `dd-runner --sqlite` | **`dd-diet-rust-sqlite`** | rust + rusqlite, executes the tick phases against SQLite |
| `dd-runner --kernel` | **`dd-diet-rust-rust`** | rust + hand-written in-RAM evaluator, zero SQLite |
| (does not exist) | **`dd-rust-dd`** | the real thing, on the `differential-dataflow` crate. NOT YOURS, see below |

"diet" is the user's word for "has the shape of dd without the algebra".
Carry the arm names into: the binary's flags, the grade ratchet
(`graded.tsv`), `ARCH.pl` task-row prose, `BENCHMARKS.md`, and any bench row
you add. A rename that leaves a stale name in the ratchet is half-done; grep
for `dd-runner`, `--kernel`, and `--sqlite` across the repo and report the
full hit list you fixed.

DO NOT build `dd-rust-dd` in this lane. It is the 260-360 line kernel priced at
`plans/2026-08-10-dd-dance-recon.PLAN.md:136-141` and it is a separate arc. Do
leave the naming slot open for it (a third arm value that errors "not built
yet"), so the bench rows have somewhere to land.

## JOB 1 (the user's "A"): a rust adapter for bench-cli
`CONTRACT.md:3-4` states bench-cli IS the gate that precedes any rust lowering,
and `CONTRACT.md:372` lists `v5 rust (dl)` as "not written -- priced in §6".
The contract already specifies one CLI shape any engine can satisfy.

Write `v6/bench-cli/adapters/` entries for the dd-runner arms following the
EXACT shape `tsv2.sh` uses. Read `tsv2.sh` and `oracle.sh` first and mirror
their contract: the same flags, the same output keys, the same
`TSV2_ORACLE_DIFF` referee convention described at `CONTRACT.md:154`.

If an arm CANNOT satisfy the contract, say exactly which contract clause it
fails and why, with the file:line. Do not fake a row. `CONTRACT.md` has a
"skipped with reason" convention already (see the `v5 rust` row); use it.

## JOB 2: the emitted-runtime bench covers all three emitters
`just dl6-bench-full` runs grid_10000, layered_10000, chain_10000 through the
EMITTED runtime. Per `BENCHMARKS.md:27-44` the emitted dl6 runtime is "the
ratchet subject". Today that means tsv2 only.

Bring the two dd_plan arms into the SAME cases with the SAME checksums. The
existing bench states its own expected values in `v6/justfile:363` (grid_10000
derived=1069200 checksum=9d7239568960d6a8) and `:379` (chain_10000
derived=9996213 checksum=df09b2f409f8b9a8). AN ARM THAT PRODUCES A DIFFERENT
CHECKSUM IS WRONG, not slow. Report a checksum mismatch as a correctness
finding and STOP that arm rather than banking a time for a wrong answer.

## JOB 3: retraction
The user asked specifically for retraction throughput at 10k. `just
dl6-dred-bench` measures in-place DRed vs refCount on a grid. Determine whether
the dd_plan arms can run that case. If yes, enter them. If no, state the
blocking clause with a citation. Do not invent a new retraction bench.

## JOB 4: wire it up the way every other bench is wired
- a row in `v6/labs/BENCHMARKS.md` following the existing row format exactly
  (purpose + history + budget), in the section its layer belongs to per the
  truth stack
- a leg in `just perf-all` following the EXISTING leg pattern at
  `v6/justfile:441-459`: a ten-word purpose header, a named run-capped budget
  (`PERF_*_S` env var with a default), its own wall time, and NON-ABORTING on
  failure. Copy the shape of the legs already there. Do not restate a command
  line that a recipe already owns; `perf-all` calls `just <recipe>`.

## FILES YOU OWN
```
v6/bench-cli/adapters/
v6/bench-cli/CONTRACT.md          (the skipped-with-reason rows + any new engine row)
v6/bench-cli/STANDINGS.md
v6/labs/BENCHMARKS.md
v6/labs/exec_shootout/dl6/        (bench cases only)
v6/justfile                        (perf-all leg + any new recipe)
```
CONCURRENT LANE `fix-zero-column-ref-target` OWNS `v6/prolog/lower.pl` and
`v6/prolog/compile/0_generic_expand.pl`. DO NOT EDIT THOSE.
DO NOT EDIT `v6/prolog/compile/6_emit_dd_plan.pl` (just landed from another
lane). If a bench needs an emitter change, STOP AND REPORT with the citation.

`v6/dd-runner/src/` is YOURS ONLY IF a contract adapter needs a flag the binary
does not expose. Prefer a wrapper script over changing the binary. If you must
change it, keep the diff to argument parsing and say so loudly in the report.

## THE 10-SECOND LAW AND ITS NAMED EXCEPTION
Any operation over 10s is a defect to investigate, not a budget to normalize.
BUT `perf-report` is explicitly NOT in green-all because 958s "violates the
10-second law by three orders of magnitude, and this is a reporting matrix, not
a gate" (`v6/justfile:415-420`). Perf batteries are run-on-demand. Your new leg
follows that same rule: it belongs in `perf-all`, NEVER in `green` or
`green-all`.

## ANTI-CHEAT TABLE
| banned | why |
|---|---|
| writing a NEW bench script | the user's whole point; enter the existing harnesses |
| a timing row for an arm whose checksum does not match | that banks a number for a wrong answer |
| restating a command line `perf-all` can reach via `just <recipe>` | the atlas says legs reuse recipes |
| putting a perf leg in `green` or `green-all` | 10-second law; perf is on-demand |
| `--no-verify` | the rail is the gate |
| claiming a number you did not run | every number is pasted tool output |
| editing files outside your list | concurrent lanes hold the rest |

## A LANE CAN EXIT rc=0 WITH A RED GATE AND ZERO COMMITS
Check your own gate output before reporting done. rc=0 is not evidence.

## GATE (run, paste output)
```
just --justfile v6/justfile bench-cli
just --justfile v6/justfile dl6-bench
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
`bench-cli` expects: `BENCH-CLI timed=16 (swipl 11 / reference 5)
disqualified=0 ungraded=0 hash-agreement=OK`, exit 0, ~5 min. If your adapters
add cells, that expected line CHANGES; update the comment at `v6/justfile:393`
that states it, and say in the report what it changed from and to.

## KNOWN RED (pre-existing, NOT yours)
`.github/CI-KNOWN-RED.md`. Read BEFORE reporting anything broken.

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose, say TODO or "not built
yet". Comments state ONLY constraints the code cannot show, no change-log
narrative, no dates, no arc references. Descriptive names, never single-letter.
Construct names use ONLY rxjs, prolog, or SQL vocabulary. Colocated
consistency inside a file.

## COMMIT OFTEN. A prior lane lost a whole run to a machine sleep.

## REPORT
`REPORT.md` at the worktree root: (1) which of the three emitter outputs now
appear in WHICH harness, as a table, (2) any arm that could not satisfy a
contract, with the failing clause cited, (3) the before/after of the bench-cli
expected line, (4) every gate command with pasted output, (5) checksum
agreement per case per arm, (6) what you did NOT do and why. Do not open a PR.
Do not spawn subagents; lanes never fan out.
