# OVERNIGHT CLEANUP AUDIT (planner contract, user go 2026-07-28)

User word: "cleanup/checkup on all the code so far since last night ...
sonnet is sloppy ... what are baselines testing and are they not redundant
false positive? have a cleaner go thru code and tests please, have it look
at the tests into the code vs the stated goals of the project."

## Scope

Every commit on this branch since 2026-07-27 20:00 (82 commits, ~17.8k
insertions; enumerate with `git log --oneline --since="2026-07-27 20:00"`).
The wave includes: P0 tracing (v6/dl/src/0_trace.ts), F7 fix + sh-parse
rework (1_hosts.ts), perf sub-spans + rowsForPath (3_runtime.ts,
4_ingest.ts), binds seam + clock bind (1_binds.ts, 6_http.ts, 0_types.ts),
the tsv2 runtime + scripts (v6/tsv2/**), the tsv2 prolog compiler
(v6/prolog/compile/**), ticklog oracle (conformance/ticklog.pl), tool
scripts (v6/tools/**), and all tests added alongside.

## Ownership split (two other agents are live; collisions are defects)

- MAY EDIT: v6/dl/**, v6/tsv2/**, v6/tools/**, v6/sprefa-store/** tests.
- REPORT-ONLY (no edits, findings instead): v6/prolog/compile/**,
  v6/prolog/conformance/** (the C2 agent owns those files right now), and
  do NOT create parse_dl.pl / print_dl.pl / dl_view/ / SYNTAX.md (phase D
  parser agent owns those names).

## The goals the code is audited AGAINST (read all before judging)

- CLAUDE.md standing laws + "Style notes for this repo" (interface-bound
  functions declared in the package header types.ts; I-prefix interfaces;
  descriptive names; banned words provenance/substrate/load-bearing/regime;
  async-becomes-rxjs-sync-stays-sync incl. the Observable-that-discards
  symptom; exactly ONE .subscribe; no Subject request/response bridges;
  N+1 write ban; recompute guard).
- plans/2026-07-27-tsv2-compile-target-header.md (incl. class-34 reuse law:
  runtime must import the NAMED store symbols, never parallel versions).
- plans/2026-07-27-v5-port-perf-header.md (tracing seams, perf line shape).
- v6/dl/scripts/goal-endurance.sh as the end-goal definition (kill -9
  mid-delay, reboot, exactly-once).

## Task A -- code audit + safe fixes

Walk the diff area by area. Apply fixes directly ONLY where mechanical and
semantics-preserving: dead code, unused exports/imports, duplicated helpers,
naming-law violations, comments that narrate instead of stating constraints,
stray console.* diagnostics outside the tracing spine, header-interface gaps
(class/fn shipped without its types.ts contract), Observable-discard shapes,
copy-paste drift between near-twin code paths (BindRunner vs HostRunner is
the prime suspect: the bind seam was written as the "input twin" of
1_hosts.ts -- verify it actually reuses shared machinery where the twin
does, and file a finding where the twinning is cosmetic). Anything semantic
(behavior could change) = ranked finding, no fix.

## Task B -- test audit (the "redundant false positive" question)

For EVERY test file added or changed in the range, produce a table row:
test name | what it actually asserts | can it fail? | verdict.
"Can it fail" is checked the hard way on suspects: temporarily revert or
break the code under test in the worktree and confirm the test goes red
(then restore). Specifically hunt:
- vacuous tests: assertions against the test's own mocks/fixtures, always-
  true assertions, tests that pass with the feature's code deleted;
- redundant tests: same behavior asserted in multiple suites with no added
  discrimination (name which one to keep and why);
- wall-clock dependence: real sleeps/intervals (the 3s reload-kills-timer
  window in 6_binds_http.test.ts is a known suspect) -- file as findings
  tied to the pending SchedulerLike-injection proposal, do not rewrite the
  timing model yourself;
- goal mismatch: stated project goals with NO test discriminating them, and
  tests asserting things no goal asks for.

## Task C -- baselines/ratchets audit

Enumerate every baseline-shaped check in the repo (known: one-subscribe.sh
baseline 1; the tsv2 import gate; no-new-eprintln ratchet (v5 side);
conformance fixture counts; scoreboard buckets; ARCH.pl go). For each:
what it guards, how it could false-positive (passes while the guarded
property is broken -- e.g. grep-shape escapes) and false-negative, and one
concrete tightening if cheap. Fix grep-shape escapes directly (mechanical);
anything else = finding.

## Validation before every commit

v6/dl: pnpm run typecheck && pnpm test (90/90 at entry) &&
sh ../tools/one-subscribe.sh (prints 1). v6/sprefa-store/js: pnpm test
(89/89). v6/tsv2: its test script (6/6) + scripts/sweep.sh unchanged
scoreboard. v6/dl scripts/goal-endurance.sh on a free port at the END of
the arc. Removing a test requires the redundancy justification in the
findings doc, and suite counts in the commit message.

## Deliverable

plans/2026-07-28-cleanup-audit-findings.md: the per-test table, the
baseline table, ranked semantic findings (most severe first, each with
file:line + failure scenario), and the list of applied fixes with commit
hashes. Worktree commits per green state; coordinator merges.
