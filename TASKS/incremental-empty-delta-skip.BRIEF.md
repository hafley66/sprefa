# incremental-empty-delta-skip

Issue: `issuectl show incremental-empty-delta-skip` (measurement + design there; the one-path-busy-tick-cost comment on it says which half is already landed). Base: `git merge --ff-only effa67c95c03667e76719354f58e6914b43ecd20` first; if the merge is a no-op because your worktree is already there, that satisfies it; failure otherwise = stop and hail. Branch `fix/incremental-empty-delta-skip`. PR to main.

## The defect
The remaining half of the issue: frontier clear, promote, and read_staged statements still run for rels whose frontier holds no row, and the wide_64 receipt (128 rels, ~15 statements per rel per tick) was never taken. #423 and #434 already gate level runs and recounts on TickWork; this arc extends the same gating to the per-rel frontier housekeeping.

## Deliverable
1. Read first: `v6/sprefa-engine-rs/src/incremental.rs` (TickWork, note_frontier_copies, the promote/clear sites; #434 just landed grew_at/shrank_at and the 4-column probe), `docs/failure-modes.md` entries 79, 83, 84, 85, `v6/prolog/conformance/rulings.pl` ruling per_rel_delta_only.
2. Planning protocol before code: signatures, pseudo-code, lifetimes, storage then read/write order, in the PR body.
3. Implement: a rel's clear/promote/read_staged run only when TickWork says its frontier was filled this tick. No new tables.
4. Receipts: COUNT test over the shared_frontier wide programs (idle tick under 2 + clock statements per rel set, and the per-rel-per-tick statement count for wide_64 before/after), tick logs byte-identical, grade.sh byte-clean unchanged, `tests/fixtures/ghcache_ticklog_base.txt` byte-identical, ghcache fold statements before/after (baseline at your base: 7,522).
5. Ledger entry (number 86; check the file first, renumber on collision). ARCH row only if a task row names this.

## You own
`v6/sprefa-engine-rs/src/incremental.rs`, `v6/sprefa-engine-rs/tests/` (additive), `docs/failure-modes.md`.
Forbidden: `v6/prolog/lower.pl` and everything under `v6/prolog/` (lane fix-delta-arm-subset-expansion owns the compiler), `v6/prolog/1_expansion.pl` and `0_coalesce_expand.pl` (lane feature-null-design-lowering), `run.rs`, `sql.rs`, `driver.rs`, `v6/dl/**`, conformance fixtures.

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 445/0
cd v6 && just plunit                                        # 1082/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=445 byte-clean=341
cd v6/sprefa-engine-rs && cargo test --workspace            # 168/0 + yours
bash v6/dl/ghcache/gate.sh                                  # ticks=14 pr_transition_open_merged=1
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Batteries in the background with `timeout`; never foreground-wait more than 10 s. Commit per item; PUSH before you report; a result with nothing pushed is not a result.

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state only constraints the code cannot show. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). Formerly-quadratic paths get COUNT tests, additive only.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: numbers"`. Blocked: one line, stop.
