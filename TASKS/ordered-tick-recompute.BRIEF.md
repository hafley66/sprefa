# ordered-tick-recompute

Issue: `issuectl show ordered-tick-recompute` (read it first; the measurement table and every line number are there).
Base: `git merge --ff-only <sha the coordinator states>` is your first action. Fail = stop and hail.
Branch: `feature/ordered-tick-recompute`. PR to main.

## You own
- `v6/sprefa-engine-rs/src/ordered.rs` (logic; a sibling lane `engine-tick-trace` is adding `Scope::verb` wrappers to the same file and lands first; when the coordinator hails you its merge sha, `git merge origin/main` before touching the file. Until then: read, design, write the COUNT test and the dirty-set type; do not edit ordered.rs.)
- `v6/sprefa-engine-rs/src/incremental.rs` (extract `reads_frontier_of` at :1004-1040 into a shared `pub fn`; no other change)
- `v6/sprefa-engine-rs/tests/ordered_statement_count.rs` (new)
- `v6/prolog/ARCH.pl` line 855 text only (the F5 sentence)
- `docs/failure-modes.md` (append one entry)

Forbidden: `run.rs`, `trace.rs`, `cost.rs`, `sql.rs`, `program.rs`, `v6/dl/**`, `v6/prolog/**` except the one ARCH line.

## The defect in one line
`ordered.rs::run_tick` reads every rel 5x and recomputes every level 2x per tick, so one arrival costs 1,135 statements + 202 batches on ghcache (154 rels); an idle tick costs the same.

## Deliverable, in this order
1. COUNT test first, red at base: fold `v6/dl/ghcache/ghcache.schedule.json` (drive it the way `tests/dl6_run.rs` does) and read `crate::sql::SEAM_TALLY.statements` before/after each tick. Assert per-tick statements for the zero-arrival ticks (ticks 9 and 10 in that schedule) are below 100, and for the one-arrival ticks below 300. Record today's numbers (1,135) in the test header as the fail-pre-fix receipt. Also assert the tick log is byte-identical to the log produced at base (store the base log under `tests/fixtures/ghcache_ticklog_base.txt`, generated once at the base sha and committed).
2. Planning protocol (CLAUDE.md): in the PR body before code, type signatures first, pseudo-code body, instance lifetimes, storage layout + read/write sequence. The shape:
   ```rust
   struct TickDirty { rels: HashSet<Arc<str>>, before: HashMap<Arc<str>, Snapshot> }
   impl TickDirty {
       fn mark(&mut self, rel: &str, rows_changed: i64, seam: &SqliteSeam, program: &GenProgram) // reads the before-snapshot lazily, once, before the first write
       fn any_of(&self, rels: &[String]) -> bool
   }
   fn levels_to_recompute(program: &GenProgram, dirty: &TickDirty, reads: &ReadsFrontierOf) -> Vec<usize>
   ```
   `rows_changed` comes back from every seam write (`QueryResult`), so the dirty bit costs zero SQL.
3. `recompute_levels` takes the dirty set and skips levels none of whose read rels is dirty; a recomputed level with `rows_changed == 0` and the same `count(*)` does not mark itself dirty. Two recompute passes stay (before and after occurrences); each uses the dirty set as it stands.
4. `read_snapshot` for the before/after diff reads dirty rels only; `build_deltas` gets the same input it gets today for those rels and an empty pair for the rest.
5. Frontier clears (`stage_ordered_frontiers` path) only for rels whose staging insert reported `rows_changed > 0` last tick. If that requires an `incremental.rs` change beyond the extraction in "you own", stop and hail with the line numbers; do not widen ownership yourself.
6. Three runs each, before and after, statements per tick for ticks 0, 5, 9; in the PR body as a table. Byte-identical tick log (item 1) and `grade.sh byte-clean` unchanged are the correctness receipts.
7. `docs/failure-modes.md` entry; `ARCH.pl:855` F5 sentence updated to the landed numbers.

## Style laws (CLAUDE.md, enforced)
No `eprintln!`; `tracing` only. Comments state constraints the code cannot show. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). No DBSP/Z-set kernel, no new algebra: the change is a dirty set over the existing SQL plan ("i only want emitters").

## Gates, all green before the PR, numbers in the PR body
```
cd v6/prolog/conformance && swipl -g go -t halt go.pl      # 444/0
cd v6 && just plunit                                        # 1065/0
bash v6/sprefa-engine-rs/grade.sh                           # graded=444 byte-clean=340, must not drop
cd v6/sprefa-engine-rs && cargo test --workspace            # 161/0 + yours
bash v6/dl/ghcache/gate.sh                                  # same ticks number as main at your merge base
cd v6 && just ghcacher-rust                                 # goldens=6
cd v6/prolog && swipl -g go -t halt ARCH.pl                 # 7/0
```
Background batteries with `timeout`; never foreground-wait more than 10 s. Commit per item. `git status` clean before you hail.

Done: `boop beep hail sprefa-coordinator --from ordered-tick-recompute --body "PR #<n>: statements/tick before->after (t0,t5,t9), gate numbers"`.
Blocked or brief wrong: hail, one line, stop.
