# v5 golden use-cases readiness — brief (opus worktree)

User (2026-07-31, going to bed): "i want the golden use cases of v5 ready
to go tho." The 9 stopping-point programs (CLAUDE.md v6 STOPPING POINT
list) must each be RUNNABLE TODAY or carry a named, priced gap. This lane
grades all 9 and fixes only small gaps.

## The 9, with current receipts to start from

1. ghcacher — `just ghcacher-golden` exists; also gains real SWR via a
   clock_period row (open follow-up from the clock-bind landing).
2. diags for LSP — `just lsp-diags` (green-all member).
3. git pre-commit --changed — NO receipt known; the watch/enumerate/sh
   hosts + extraction-live rig are the parts. If a small program spells
   it, write it as a fixture + recipe; if a construct is missing, name it.
4. sprefa-extract run — `just extraction-live` + flagship rig cover parts;
   grade what "the full scan/scanwork + repo/rev extraction" still lacks.
5. auto-synced repo list — grade: expressible with enumerate/watch hosts?
6. v5 bench parity — `just multirepo-golden` + SCALE/PERF receipts; state
   the current distance number, do not chase it in this lane.
7. rtkq examples — `just rtkq-golden` exists.
8. file watcher scaling — watch bind landed; grade scaling claim only
   (receipt for a large tree, e.g. enumerate over the full repo).
9. standardized tick-log format — the cross-target contract; state where
   it is pinned (json_ticklog ruling + dl6_oracle) and what a second
   consumer needs.

## Task

- RUN each existing recipe; record pass/fail + wall time.
- Produce v6/READINESS.md: one row per program — status
  (READY / SMALL-GAP-FIXED / GAP(named, priced)), the receipt command, and
  the gap story. Table first, stories one line each.
- Fix ONLY small gaps in-lane (a missing recipe wrapper, a program file
  that compiles today, a stale path). Anything needing new constructs or
  runtime seams = a priced row, not code.
- Program 3 (pre-commit --changed) is the most likely writable-today win:
  try a real .dl6 (changed lines via sh host over `git diff`, gating rels)
  graded run-twice; if it works, wire `just precommit-changed`.

## Receipts required

- READINESS.md committed with all 9 rows evidenced by your own runs.
- Any new fixture: oracle-vs-emitted byte identity both doors.
- Battery on exit: conformance, sweep both modes, plunit, text-door
  (only if compiler files were touched — they should NOT be).

## Fences

- Touch: v6/READINESS.md, new .dl6 fixture(s) + rail script(s) + justfile
  recipes ONLY.
- Do NOT touch: v6/prolog/** compiler files, 0_refusal_messages.pl,
  registry/lower/emit (concurrent lanes own them), DEVLOG/self-map rails,
  labs/**.
- git worktree, base sha stated at dispatch; verify with rev-parse FIRST,
  STOP AND REPORT on mismatch. Commit per step with `git commit -n`; do
  not push.
