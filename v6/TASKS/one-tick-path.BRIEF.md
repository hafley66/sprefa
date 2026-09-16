# one-tick-path

Issue: `issuectl show one-tick-path`. Plan (read first, it is the spec in pictures): `plans/2026-08-23-one-tick-path.PLAN.visual.human.unga.md`. Decision row: `v6/prolog/conformance/rulings.pl` `per_rel_delta_only`. ARCH row: `one_tick_path`.
Base: `git merge --ff-only <sha the coordinator states>` first. Fail = stop and hail.
Branch: `feature/one-tick-path`. PR to main.

## The defect
`emit_rust.pl:216` `ordered_program/1`: ONE edge statement of kind `ordered_arrival`/`ordered_departure` (an arm reading `pre/1`) flips the whole emitted module onto `ordered.rs::run_tick`, where every `<-` level is rebuilt from base tables (`recompute_sql`) and every rel is snapshotted and diffed in Rust. #423 added a dirty set to that loop; the loop itself is the defect. Programs with no such arm already run `incremental.rs` (frontier-driven levels, `apply_edges`, `promote_frontiers`).

## Deliverable
Sequenced arms run INSIDE the incremental path. Levels stay frontier-driven everywhere. `ordered.rs` is deleted. The language surface, the oracle, and every tick log are untouched.

1. Read first, write nothing: `program.rs:170-90` (`run_tick` phase order), `incremental.rs` (`apply_levels_before_edges`, `recompute_levels_before_edges`, `apply_edges`, `apply_levels_after_edges`, `promote_frontiers`, `stage_ordered_frontiers`, `stage_departures`), `ordered.rs` (`run_tick`, `apply_occurrence`, `read_carry`, `read_departures`, `outside_occurrences`, `level_occurrences`, `carry_additions`, `snapshot_pre`), `lower.pl:3948-3990` (why `ordered_arrival` exists; `pre/1`), `conformance/rulings.pl` `one_pick_order` (pick inside a tick = arrival order, both doors).
2. Design in the PR body BEFORE code, planning protocol from CLAUDE.md: type signatures, pseudo-code bodies, instance lifetimes, storage layout then read/write sequence then uniqueness conditions. The shape to price:
   ```rust
   /// One arm's occurrences, walked one at a time only when the arm reads pre/1.
   enum ArmSchedule { SetAtOnce, Sequenced }
   fn arm_schedule(statement: &EdgeStatement) -> ArmSchedule   // from the edgestmt kind, per ARM, never per module
   fn apply_edges(seam, edges, relations, dirty: &mut TickDirty) // existing; Sequenced arms call apply_occurrence per row in arrival-index order, SetAtOnce arms keep today's set statement
   ```
   `__pre_<rel>` for a sequenced arm = the rel's table as of the start of the arm's walk, which is the frontier-promoted state after `apply_levels_before_edges`; state in the design whether that equals today's `snapshot_pre` on every conformance fixture that uses `pre/1` (13 of them, listed in ARCH:752), and cite the fixture names.
3. Emitter: `emit_rust.pl:216-270` stops setting a module flag; each edgestmt carries its schedule. `emit_ts.pl` output for unchanged programs must stay byte-identical (tsv2 is paused; grep `ordered_program` there and leave the TS door alone unless a shared predicate forces a touch, then say so).
4. Runtime: arms with `Sequenced` walk occurrences inside `apply_edges`; keyed writes stage into the frontier like every other write; `promote_frontiers` produces the deltas; no Rust-side snapshot diff. Delete `ordered.rs`; `program.rs:180` loses the branch.
5. Receipts, all additive, all in the PR body with three runs each:
   - tick log byte-identical: `tests/fixtures/ghcache_ticklog_base.txt` unchanged, `tests/ordered_statement_count.rs` caps LOWERED to the new measurement (state old/new per tick), idle tick under 10 statements
   - `grade.sh` `byte-clean=340` unchanged (this is the corpus-wide oracle comparison; many fixtures have `<+`)
   - a COUNT test that a program with one `pre/1` arm and 50 `<-` levels does not recompute a level whose inputs did not move (EXPLAIN or statement count, not end state)
   - `DL_TRACE_SUMMARY` table with `unlabelled` calls = 0 inside ticks
6. Ledger entry in `docs/failure-modes.md` (next free number); ARCH `one_tick_path` row flipped to done with the numbers; `plans/2026-08-23-one-tick-path.PLAN.visual.human.unga.md` gets a "Landed" line at the top, nothing else edited there.

## You own
`v6/sprefa-engine-rs/src/{ordered.rs,incremental.rs,program.rs,run.rs}`, `v6/sprefa-engine-rs/tests/**`, `v6/prolog/emit_rust.pl`, `v6/prolog/lower.pl` (only if the edgestmt kind needs a field), `v6/prolog/ARCH.pl` (one row), `docs/failure-modes.md`, the plan doc (one line).
Forbidden: `v6/dl/**`, `emit_ts.pl`, `parse_dl_dcg.pl`, `analyze.pl`, `registry.pl`, conformance fixtures (if a fixture must change, the design is wrong; hail).

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state constraints the code cannot show. No em dashes. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). No new kernel, no Z-set algebra: the frontier tables ARE the delta ("i only want emitters").

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 444/0
cd v6 && just plunit                                        # 1076/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=444 byte-clean=340
cd v6/sprefa-engine-rs && cargo test --workspace            # 163/0 + yours
bash v6/dl/ghcache/gate.sh                                  # ticks=14 pr_transition_open_merged=1
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Batteries in the background with `timeout`; never foreground-wait more than 10 s. Commit per item; PUSH before you report; a result with nothing pushed is not a result.

Done: `boop beep hail sprefa-coordinator --from one-tick-path --body "PR #<n>: statements/tick before->after, gate numbers"`; if the hail is refused, message the session named sprefa-* over the cross-session socket.
Blocked or design fork: hail/message, one line, stop. Design forks come back as cited options; the user decides ("lang design happens with Chris in the room").
