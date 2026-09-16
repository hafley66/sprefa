# recount-call-volume

Issue: `issuectl show recount-call-volume`. Base: your worktree is cut at or past 55dad1577a9d3c0371b880bc3dcf4377c15b1ecc; verify with `git merge-base --is-ancestor 55dad1577a9d3c0371b880bc3dcf4377c15b1ecc HEAD` (an ff-merge no-op counts as satisfied). Branch `fix/recount-call-volume`. PR to main.

## The defect
ghcache 14-tick fold: `recount` is 3,244 calls (~232/tick against 109 rules), 18.7 ms of the 191 ms wall. Two causes to attack, in order:
1. **Negated-loss eligibility is now dead weight.** #434's gate (incremental.rs `recount_needed`) keeps a head recount-eligible when a NEGATED input LOST a row, because no delta insert arm existed for that case. #435 (`lower.pl level_negative_delta_arms`, merged) emits that arm. Drop `shrank_at` from the negated check; the arm covers it. The pinned dependency case: fixture `callgraph_unused_inverts_with_the_call_set` tick 4 (`-call('b.rs',main)` must add `unused(main)`) must stay byte-clean with `DL_NO_SHRINK_GATE` UNSET.
2. **Per-round refire.** `sequence_level_rounds` re-runs eligible recounts every recursion round. Measure how many of the 3,244 are round-2+ refires with no new shrink between rounds; if a head's inputs did not shrink SINCE ITS LAST RECOUNT (the same monotone-clock reading level_runs uses, LevelPhase::Recount already keys ran_at), the refire is a no-op statement. Gate it the same way. If measurement shows rounds are already gated and the volume is genuine shrink traffic (ghcache retracts answered http demands every tick), say so with numbers and stop at cause 1.

## Read first
`v6/sprefa-engine-rs/src/incremental.rs` (recount_needed, recount_runs_this_tick, note_run/settle_level_run, sequence_level_rounds), `docs/failure-modes.md` 83/85/86/88, `v6/prolog/conformance/rulings.pl` per_rel_delta_only, `tests/recount_gate.rs`.

## You own
`v6/sprefa-engine-rs/src/incremental.rs`, `v6/sprefa-engine-rs/tests/` (additive), `docs/failure-modes.md` (next free ledger number; renumber on collision), `v6/prolog/ARCH.pl` (close the delta_arm_subset_expansion compatibility note if you retire it).
Forbidden: `v6/prolog/lower.pl`, `v6/dl/**`, conformance fixtures except additive, `run.rs`, `sql.rs`, `driver.rs`.

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 445/0
cd v6 && just plunit                                        # 1088/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=445 byte-clean=341
cd v6/sprefa-engine-rs && cargo test --workspace            # 172/0 + yours
bash v6/dl/ghcache/gate.sh                                  # ticks=14 pr_transition_open_merged=1
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Receipts: recount calls 3,244 -> ?, fold statements 6,738 -> ?, wall 191 ms -> ? (target: close on 152), `ghcache_ticklog_base.txt` byte-identical, recount_gate.rs extended with a negated-loss-skipped COUNT case. Three runs per measurement. Batteries in background with `timeout`; no foreground wait over 10 s. Commit per item; PUSH before you report.

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments only for constraints code cannot show. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). COUNT tests additive only.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: numbers"`. Blocked: one line, stop.
