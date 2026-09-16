# BRIEF: dd-runner rust x rust kernel, remove the three naive shapes

## Base
- Branch: `feature/dd-kernel-denaive`, worktree of `/Users/chrishafley/projects/sprefa`.
- Base sha: `4dd8ef3a` (main). Verify with `git log --oneline -1` FIRST.
  Any other base = STOP AND REPORT.

## One sentence
`v6/dd-runner/src/kernel.rs` is the rust x rust arm, the speed reference the
production arm is measured against, and it is 215 lines of naive shapes; make
it fast without changing what it computes.

## The three defects, cited

| # | site | shape |
|---|---|---|
| 1 | `kernel.rs:86-107` | clones state and re-derives from base every round |
| 2 | `kernel.rs:160-177` | full cross product |
| 3 | `kernel.rs:109-112` | linear `Vec::contains` membership |

Defect 1 is the same shape the SQLite side already fixed: the DD closeout
measured a separate rederive pass at 807.70 ms, 48.0% of a traced run, and
fork 1 (timestamped signed-delta fixed point) removed it there. It landed at
`v6/sprefa-store/src/engine.rs:642`, `:724`, `:762`. Read those three sites
before you write anything: the same idea, propagate only threshold-crossing
`+1` and `-1` membership changes, is what defect 1 wants.

## HARD CONSTRAINT: this is a speed change, not a semantics change
`v6/dd-runner/grade.sh` grades fixtures byte-clean against the tick-log oracle.
Every commit keeps every currently-passing fixture byte-clean. A byte that
moves is a defect in your change, never a fixture to update.

Get the BEFORE numbers first, on your own base, before touching a line:
fixture count byte-clean, wall time per fixture, peak RSS per fixture. Every
later claim is a delta against those.

## Work
1. Baseline. Numbers above, plus `cargo build --release` and `./grade.sh` on an
   unmodified tree.
2. Defect 3 first: it is the smallest and independent. Replace linear
   membership with a hashed or sorted structure. Measure.
3. Defect 2: the cross product. Establish what it is joining and on what key,
   then index the right side and probe it. Measure.
4. Defect 1 last: it is the largest and the other two make it easier to read.
   Carry per-key refCount and propagate only threshold crossings. Measure.
5. One commit per defect, each with its own before/after numbers.

## What NOT to do
- Do not add a dependency without the build-vs-buy analysis the repo requires:
  library research and a written candidate-by-candidate comparison BEFORE any
  hand-rolled claim. `differential-dataflow` and `timely` themselves are the
  obvious candidates; if you rule them out, the comparison is written down, not
  asserted in one line.
- Do not change `v6/dd-runner/src/main.rs`. Another lane owns it right now,
  implementing the twelve tick phases on the SQLite arm. Touching it will
  conflict.
- Do not change the emitted plan format or anything under `v6/prolog/`.
- Do not chase `dd_plan`'s mutual-recursion stop at
  `v6/prolog/compile/6_emit_dd_plan.pl:460-470`. Out of scope.

## Files you own
| path | permission |
|---|---|
| `v6/dd-runner/src/kernel.rs` | full |
| `v6/dd-runner/Cargo.toml` | only if step 1's analysis justifies a dependency |
| `plans/2026-08-11-dd-kernel-denaive.md` | create |

Forbidden: `v6/dd-runner/src/main.rs`, `v6/dd-runner/grade.sh`,
`v6/prolog/**`, `v6/boop/**`, `v6/tools/**`, `.github/**`, `chat_log/**`.
Three other lanes are live in those trees.

## Gates, every commit
```bash
cd v6/dd-runner && cargo build --release
cd v6/dd-runner && ./grade.sh          # byte-clean count may only go UP
cd v6/dd-runner && cargo clippy --all-targets -- -D warnings
cd v6/dd-runner && cargo fmt --check
```
The 10-second law: any single operation over 10s is a defect to investigate,
never a budget. If a fixture crosses 10s, that is a finding you report.

## Deliverable
`plans/2026-08-11-dd-kernel-denaive.md` with:
1. The baseline table: fixture, byte-clean, wall ms, peak RSS, on an untouched
   tree.
2. One section per defect: what it was, what it became, the delta table, the
   commit.
3. Any dependency decision, with its written candidate comparison.
4. Gate output verbatim.
5. A closing table: total speedup per fixture, and the ratio to the SQLite arm
   if `grade.sh` gives you one.

## Style laws, inline so you need no judgment
- No em dashes. No `provenance`, `substrate`, `load-bearing`, `regime`.
- "refusal" is banned in prose; unbuilt work is "TODO" or "not built yet".
- Comments state only constraints the code cannot show. No change-log
  narrative, no dates, no arc references, max 2 consecutive comment lines.
- Construct names use rxjs, prolog, or SQL words only. "support" is banned,
  the word for a per-key membership count is refCount.
- Tables and lists over prose. Numbers come from tool output only.
