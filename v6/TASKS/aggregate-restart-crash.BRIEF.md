# aggregate-restart-crash

A resident `dl6 run` restart against the existing one-db (`~/.agent/dl6.db`) panics in tick 1: `incremental.rs:1828 aggregate scope batch failed: SqliteFailure(.., Some("malformed JSON"))`. A fresh db never crashes (the 14-tick gate is green). Restart durability is a standing law (self-diagnosis after SIGKILL; #415 restart keeps ETags), so this is a blocking defect. Base: worktree at or past c0c39ef34b7ea64a8abb28a69077853409884e9b. Branch `fix/aggregate-restart-crash`. PR to main.

## Deterministic reproduction, seconds, no guessing
```
cp /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/ec1f17ba-6f12-4bd9-b6ac-feb07cde818e/scratchpad/dl6-crash-copy.db /tmp/crash.db
cd v6/dl/ghcache && DL6_DB=/tmp/crash.db RUST_BACKTRACE=1 GITHUB_TOKEN=$(gh auth token) \
  ../../sprefa-engine-rs/target/release/dl6 run $PWD/ghcache.dl6
```
(the copy is a snapshot of the live db written by the pre-#439 program; the crash needs the live token because tick 1's arrivals participate. If you can reproduce with a scripted schedule instead, prefer that and commit it as the fixture.)

## Deliverables, in order
1. DIAGNOSABILITY FIRST: `apply_aggregate_level_statement`'s `.expect("aggregate scope batch failed")` (incremental.rs:1828 and the sibling expects) names neither the head rel nor the statement. Replace with an error path that carries head_rel, the statement index, and the first 200 chars of the SQL. That alone turns this class of bug from a hunt into a read. Same treatment for the other bare expects in the aggregate path.
2. Root-cause with the new message. Leading hypothesis, unverified: a restart-with-ETags tick answers 304s whose empty/absent body reaches a json function that a fresh run only meets after a 200 seeded `last_body` (coordinator checked: the STORED db has zero json-invalid and zero empty bodies, so the poison value materializes during tick 1, plausibly an empty or NULL operand to json_* in a scope seed). Verify against the message, do not trust the hypothesis.
3. Fix in the engine or lowering guard (e.g. json functions over host body columns guard NULL/'' the way decode already survives null, PR #417). Language surface unchanged.
4. Fixture: a cargo test folding a program to a FILE db (schedule with a 200), then re-running the binary against that same db with a 304/empty follow-up schedule; red before the fix with the OLD panic, green after. docs/failure-modes.md entry (next free number; 91 is taken). ARCH row only if named.

## You own
`v6/sprefa-engine-rs/src/incremental.rs`, `src/bin/dl6.rs` if the guard sits there, engine tests, `docs/failure-modes.md`. Forbidden: `v6/prolog/**` EXCEPT a lowering guard if the fix is provably lowering-side (then grade byte-clean rules: 445/341 must hold or every moved fixture named), `v6/dl/**` beyond a new test schedule file, conformance fixtures.

## Gates before the PR
conformance 445/0; plunit 1114/0; grade 445/341; cargo all/0 + yours; ghcache gate ticks=14 account_ticks=14; goldens 6; ARCH 7/0. Plus the reproduction above running clean to its first resident tick, output pasted in the PR body. Batteries in background with timeout; no foreground wait over 10 s. PUSH before reporting.

## Style laws (CLAUDE.md)
tracing only, comment budget, no em dashes; banned words: provenance, substrate, load-bearing, regime, ground truth (oracle), refusal, support (refCount).

Done: `boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: numbers + repro-clean receipt"`. Blocked: one line, stop.
