# SCOUT: dd / timely / dbsp — how they relate, and what they test

Research + implementation lane. You are a scout: your PRIMARY deliverable is
knowledge with citations, and a SECOND deliverable is as many new bench cases
as you can land without guessing.

## FIRST ACTION, NON-NEGOTIABLE
```
git merge --ff-only 9ecd3341
```
Failure or missing trees = STOP AND REPORT.

## WORKTREE SETUP BEFORE FIRST COMMIT (never `--no-verify`)
1. copy `v6/sprefa-extract/target/release/extract` from the main tree in
2. `cd v6/tsv2 && pnpm install`
3. `cd v6/sprefa-store/js && pnpm install`

## THE USER'S ASK, VERBATIM
"we should have a codex luna scout out dd/timely/dbsp how they relate and
principal test cases for completeness and performance from their codebases and
reimpl them here and compare in our benches as new classes like reach and cycle
and grid etc. shootouts"

## WHAT THIS REPO ALREADY HAS (do not re-derive, verify and build on)
| crate | version | where | role today |
|---|---|---|---|
| `differential-dataflow` | 0.25 | `v6/sprefa-store/Cargo.toml` | `src/oracle.rs` `oracle::dd_reach`, the correctness ground truth + resident-RAM yardstick |
| `timely` | 0.31 | same | dd's runtime |
| `dbsp` (Feldera) | 0.323 | same, OPTIONAL `with-dbsp` | blocked: triggers a rustc ICE on the repo's default nightly; builds on stable with `RUSTFLAGS="-C link-arg=-fuse-ld=lld" cargo +stable build --features with-dbsp --example dbsp_reach` |
| `salsa` | 0.28 | same | third IVM implementation, red-green, cross-checked in `tests/reconcile.rs` |

Existing bench harnesses and their CURRENT case classes:
| harness | recipe | case classes today |
|---|---|---|
| rust shootout (interp / rxgraph / mono) | `just shootout` | chain, grid, layered at 10k/100k/1M |
| dl6 emitted bench | `just dl6-bench-full` | grid_10000, layered_10000, chain_10000 |
| dl6 retraction | `just dl6-dred-bench` | grid 45x45, in-place DRed vs refCount |
| store rig | `just bench` | DAG + cyclic ladders |
| PERF-REPORT | `just perf-report` | 7 engines x 10 scales, DAG |

READ `v6/labs/BENCHMARKS.md` (297 lines, the atlas) BEFORE ANYTHING. Its
"truth stack" at :27-44 already ranks these layers. Your work extends that
document, it does not replace it.

## THE VOCABULARY, GET THIS RIGHT IN EVERY LINE YOU WRITE
- **Z-set** (Z-relation) is THE ALGEBRA: a relation whose rows carry integer
  weights; a negative weight is a retraction.
- **DBSP** is the circuit theory over Z-sets (differentiate / integrate).
- **Feldera** is the COMPANY shipping the `dbsp` crate. "Feldera algebra" is
  not a thing; do not write it.
- **Differential Dataflow** is McSherry's system: weighted multisets over
  PARTIALLY-ORDERED timestamps, riding Timely Dataflow.

## JOB 1, RESEARCH, THE PRIMARY DELIVERABLE
Answer these with citations to the upstream source, file + line or permalink:

1. **How do dd, timely, and dbsp relate?** What does timely provide that dd
   builds on. Where do dd and dbsp AGREE (the Z-set core) and where do they
   DIVERGE (time model: dd's partial-order timestamps + arrangements vs dbsp's
   nested circuit clocks). One table, not prose.
2. **What does each project's own test suite consider the principal cases?**
   Go read their repos. For each of dd, timely, dbsp: list the canonical
   correctness cases and the canonical benchmarks THEY ship, with the path in
   their repo. Name the ones about: reachability, cycles/SCC, grids, joins,
   aggregation, retraction/negative weights, and incremental update latency.
3. **Which of those does this repo NOT test?** Cross this against
   `BENCHMARKS.md` and the case classes table above. That gap list is the
   point of the whole lane.

## JOB 2, IMPLEMENTATION, AS FAR AS YOU HONESTLY GET
Add new case classes to the EXISTING shootout harness
(`v6/labs/exec_shootout/`), following the shape its current cases use. Target
classes the user named plus what job 1 finds: **reach**, **cycle**, grid
(exists, verify), and any upstream-canonical case your research says is
principal.

RULES:
- New cases enter the EXISTING harness protocol. Read how chain/grid/layered
  are defined and generated first, and mirror it. A new bench script is a
  FAILED lane.
- Every new case states its expected derived-row count and checksum, the way
  `v6/justfile:363,379` states them for grid_10000 and chain_10000. A case with
  no expected value is not a test.
- If a case cannot be expressed in the harness, say which harness assumption
  blocks it, with the file:line. Do not force it.

## JOB 3, THE dbsp UNBLOCK, ONLY IF CHEAP
`BENCHMARKS.md:283-289` records an open item: the dbsp head-to-head is blocked
because `dbsp_reach` needs the `with-dbsp` feature on stable and is not part of
`cargo build --examples`. If you can land the head-to-head cheaply, do it. If
it costs more than an hour, LEAVE IT and say so. Do not sink the lane here.

## FILES YOU OWN
```
v6/labs/exec_shootout/          (new cases + harness case definitions)
v6/labs/BENCHMARKS.md           (new rows only, existing row format)
plans/2026-08-12-zset-scout.md  (your research deliverable, new file)
```
CONCURRENT LANE `fix-zero-column-ref-target` owns `v6/prolog/lower.pl` and
`v6/prolog/compile/0_generic_expand.pl`. A bench lane may also be dispatched
against `v6/bench-cli/` and `v6/justfile` — DO NOT EDIT `v6/bench-cli/**`.
If you need a `v6/justfile` recipe, STOP AND REPORT the recipe you want rather
than editing that file.

## BUILD-VS-BUY LAW APPLIES AND IS THE POINT
Never assert "write our own" for a common-shaped problem without library
research and a written candidate-by-candidate analysis. No one-line dismissals.
`plans/2026-08-11-dd-kernel-denaive.md:100-107` is the house example of the
format: six candidates, each with a specific technical reason and a compile
receipt. Match that standard.

## ANTI-CHEAT TABLE
| banned | why |
|---|---|
| a claim about an upstream project with no citation | the whole deliverable is citations |
| reading a README and reporting it as what the code tests | "comments are not the language"; cite the test file |
| a new bench script | enter the existing harness |
| a case with no expected row count / checksum | that is not a test |
| writing "Feldera algebra" | wrong name; the algebra is Z-sets |
| claiming a number you did not run | every number is pasted tool output |
| `--no-verify` | the rail is the gate |

## THE 10-SECOND LAW
Any operation over 10s is a defect to investigate, NOT a budget to normalize.
Named exceptions: SCIP indexing, and the perf batteries which are explicitly
run-on-demand and NOT in `green`/`green-all` (`v6/justfile:415-420`). Your new
cases belong in the perf battery, never in `green`.

## GATE (run, paste output)
```
just --justfile v6/justfile shootout
cd v6/prolog && swipl -g go -t halt ARCH.pl
```
The shootout expects chain 10k mono fixpoint rows/sec ~7e7 (`v6/justfile:381`).
Report what you measured against that.

## STYLE LAWS
No em dashes. Banned in prose AND identifiers: `provenance`, `substrate`,
`load-bearing`, `regime`. "refusal" banned in prose. Comments state ONLY
constraints the code cannot show, no change-log narrative, no dates.
Descriptive names, never single-letter. Tables over paragraphs. Docs open with
a TOC.

## COMMIT OFTEN. A prior lane lost a whole run to a machine sleep.

## REPORT
`plans/2026-08-12-zset-scout.md` is the deliverable, with a TOC. Sections:
(1) dd/timely/dbsp relation table with citations, (2) each project's principal
cases with repo paths, (3) THE GAP: what they test that we do not, (4) what you
implemented and its measured numbers, (5) what you could not implement and the
blocking assumption cited, (6) the dbsp unblock verdict. Plus `REPORT.md` at
the worktree root with the gate transcripts. Do not open a PR. Do not spawn
subagents; lanes never fan out.
