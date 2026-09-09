# Lane `fix/extract-ts-mutation-finding` (glm53): F4 ts duplicate def keeps its edge

First action: `git merge --ff-only fe2a5e479e76f7d53dd2b943e206230358923b71`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Battery finding F4 green: a duplicate ts def must flip the corpus_unique edge
to ABSENT (today it keeps it) — un-ignore the F4 test, red receipt, fix the ts
resolver's uniqueness count, battery green, ts ratchet leg holds.

## Pattern to follow

PR #624 (python F5) and PR #626 (go F2, package-block scoping in
`go_call_name_match`) fixed the same invariant; read both diffs first. The ts
leg lives in `v6/sprefa-extract/src/lang/ts.rs` (`call_name_match` shape) —
find where corpus uniqueness is counted and why a second def does not kill it.
The mutation and expected rows are IN `tests/90_mutation_battery.rs` (F4).
ts has a module plane; a module-plane-resolved edge (origin `module_plane`)
must be UNTOUCHED by this change — only `corpus_unique` origin rows are in
play, and the origin-conservation test is the bystander guard.

## Files you own

- `v6/sprefa-extract/src/lang/ts.rs`
- `v6/sprefa-extract/tests/90_mutation_battery.rs` (un-ignore F4 only, never
  weaken an assertion)
- FORBIDDEN: rust/go/python lang files, ts_checker.rs, tests/bench/**,
  RATCHET.tsv, src/types.rs.

## Receipts (PR body, measure signature from COMMON.md on every percent)

1. Red output of un-ignored F4 pre-fix; full battery green post-fix.
2. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.
3. Gate, AFTER beeping the coordinator for go
   (`boop beep --no-wait --as fix-extract-ts-mutation-finding sprefa-coordinator "ready for ts leg"`):
   ONLY the ts leg: `cargo test --release --features cli --test ratchet_recall
   -- --exact --ignored --nocapture ratchet_ts5`. All ts5 rows hold
   (recall floor 92.07% = 48,928/53,138 codeql edge-rows). A floor break =
   the dropped duplicates carried real matches: STOP and report.
4. diff-stat = 2 owned files.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Commit per step; push; `gh pr
create`; beep the coordinator with the PR number.
