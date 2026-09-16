# feature/signed-delta-retraction: dd fork 1 as a DRed sibling in sprefa-store

## Decision context (coordinator verdict 2026-08-10, user delegated with a
## noted laze-out caveat; NOT a user-worded design row)
Build the timestamped signed-delta fixpoint as a SIBLING retraction path next
to DRed in v6/sprefa-store. DRed stays. Selection is a plain engine method,
no plan fields. Evidence: plans/2026-08-10-dd-source-hunt.RECON.md (two-pass
split 51.8% over-delete / 48.0% rederive, 99.79% of retraction wall; floor
estimate ~875ms of 1682ms).

## Files owned by this lane (nothing else)
- v6/sprefa-store/src/engine.rs: new method `retract_signed_delta` beside the
  existing two-pass DRed path (`v6/sprefa-store/src/engine.rs:350-461`,
  `:543-690` are the DRed reference; do not modify them).
- v6/sprefa-store/tests/agreement.rs: extend.
- v6/sprefa-store/examples/perf_report.rs: one new bench row.
- Optional: v6/sprefa-store/PERF-REPORT.md appendix section with the number.

## The algorithm (from the recon; follow it exactly)
Per outer tick, ONE signed pass, no over-delete cone, no rederive walk:
- Tables: `delta(round INTEGER, key INTEGER, diff INTEGER)` consolidated by
  (round, key); a persistent `refcount(key INTEGER PRIMARY KEY, n INTEGER)`
  surviving across outer ticks. INTEGER keys only (surrogate-keys law).
- Round 0: seed the direct effect of the retracted base rows as negative
  diffs.
- Round N: read ONLY round N deltas, join through the indexed dependency
  table (parent -> child), GROUP BY child SUM(diff); apply to refcount;
  a key crossing n>0 -> n=0 emits (child, -1) into round N+1; a key crossing
  0 -> n>0 emits (child, +1) into round N+1.
- Stop when round N+1 is empty. refcount persists for the next outer tick.
- Set-based SQL per round, never per-row statements (N+1 law).

## THE CYCLE TRAP (the whole reason this needs care)
Naive support counting is WRONG on cyclic graphs: a cycle sustains its own
count and rows that should die stay alive. The repo has a banked NO row for
sqlite-count on exactly this (phantom-cycle failure). The per-ROUND delta
table is what makes this variant correct: support changes propagate by
derivation round, so cyclic self-support cannot refresh itself. Your
agreement test MUST include a cyclic fixture (a reachability graph with a
cycle whose entry edge is retracted) where the dead cycle members are
removed; assert byte-identical final state against BOTH the DRed path and
the dd oracle (tests/agreement.rs already byte-diffs 3 ways; add this as a
new case, and add a fail-first note in the test header showing naive
counting keeps the cycle alive).

## Validation gate
```bash
cd <worktree>/v6/sprefa-store && cargo test            # all existing + new green
cd <worktree>/v6/sprefa-store && cargo run --release --example perf_report
```
Record dred vs signed_delta wall ms for the retraction benchmark in the PR
body and PERF-REPORT.md. No benchmark step may exceed 10s.

## Commit rail (commit-or-report)
- Commit ON THE BRANCH before exiting, up to 3 commits, prefix `rust:`.
- If blocked, write FAILURE-REPORT.md at the worktree root with the exact
  failing command + output, exit nonzero. NEVER --no-verify.

## Style laws
- No eprintln! in src/**; tracing only.
- Comments state only constraints code cannot show; max 2 consecutive
  comment lines in new code (hook-enforced). Fail-first receipts in TEST
  headers are the allowed exception.
- Banned words in prose and identifiers: provenance, substrate, load-bearing,
  regime, refusal. "support" is banned as a name -> refCount/refcount.
- Read the repo skills .claude/skills/sql-relational-design and
  .claude/skills/sqlite-costs before writing any DDL.

# ROUND 2 (round 1 = f5f2eaf4..8676a752, KEEP all of it, especially the tests)
Round 1's `retract_signed_delta` re-walks surviving reach and republishes the
weight column every retraction; it beat DRed by only 4.8%. The appendix claim
"cycle-correctness costs the re-walk" is FALSE as a general statement:
differential dataflow is the existence proof (per-ROUND counts, no survivor
re-walk; see plans/2026-08-10-dd-source-hunt.RECON.md fork 1 trace). Round 2
implements the actual mechanism:
- refcount rows carry the ROUND (derivation depth) of each support
  contribution, or equivalently store per-key min-round + count-at-round; a
  cut cycle cannot self-sustain because a member's support must come from a
  round STRICTLY BELOW its own first derivation.
- Retraction seeds negative diffs at the affected keys' rounds; propagation
  touches ONLY keys reachable from the delta, never the surviving corpus. No
  statement may scan or rewrite rows whose support did not change. Add a
  statement-count test: retracting one leaf edge of the 960k DAG executes
  O(cone) work — assert the changed-row count, not just end state.
- If depth-tracking genuinely cannot avoid the re-walk in SQL, STOP and write
  FAILURE-REPORT.md quoting the exact SQL obstacle; do not ship another
  re-walk variant.
Gate, commit rail, style laws: as round 1. Update the PERF-REPORT appendix
with round-2 numbers beside round-1.
