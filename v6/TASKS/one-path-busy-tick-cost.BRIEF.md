# one-path-busy-tick-cost

Issue: `issuectl show one-path-busy-tick-cost` (every number is there). Related: `incremental-empty-delta-skip`, `decoded-delta-from-stored`, `ordered-next-frontier-clear` (fold what applies into this arc; close them in the PR body with a sentence each or leave them with a sentence why not).
Base: `git merge --ff-only <sha the coordinator states>` first; fail = stop and hail. Branch `fix/one-path-busy-tick-cost`. PR to main.

## The defect
#427 put every program on the incremental path and made ghcache's fold 1.9x slower in wall time (7,113 -> 16,655 statements; 150 -> 286 ms). The idle tick (3 statements) is right. The busy tick is not: `recount` 8,279 calls / 48 ms with no skip, `level_insert` 1,873 calls at 45 us each where the old rebuild cost 10 us on the same tiny tables, and the per-rel fixed verbs (`publish`, `clear`, `stage`) run for rels that did not move.

## Measure first, every time
```
cd v6 && swipl --stack_limit=12G -q -l prolog/compile.pl -l prolog/emit_rust.pl -g "compile_dl6('$PWD/dl/ghcache/ghcache.dl6','/tmp/gh.rs',[emitter(emit_rust:emit_program)])" -g halt
cargo build --release --manifest-path sprefa-engine-rs/Cargo.toml --bin emit_rust_harness
DL_ADAPTERS_DIR=$PWD/dl/ghcache DL_TRACE_SUMMARY=1 sprefa-engine-rs/target/release/emit_rust_harness /tmp/gh.rs dl/ghcache/ghcache.schedule.json --final 2>&1 >/dev/null | grep -A400 'DL_TRACE_SUMMARY =='
```
Three runs per arm; the per-verb table (us, calls) in the PR body before and after every step.

## Deliverable, in order, one commit each with its own before/after table
1. `recount` runs only for heads whose `level_insert` reported `rows_changed > 0` this tick (the seam returns it). Expect 8,279 -> hundreds.
2. `publish` / `clear` / `stage` only for rels in the tick's moved set (`TickWork` from #427 already holds it; `incremental.rs` `promote_frontiers`, `stage_departures`, `stage_ordered_frontiers`, the next-frontier clear at `incremental.rs:902-908`).
3. `level_insert` cost per call: read the emitted delta SQL for one ghcache level (`levels[i].insert_sql`), EXPLAIN it (`DL_EXPLAIN=1 RUST_LOG=sprefa_engine_rs::explain=info`), and state why 45 us. If it is the `__count` refcount join, price "rebuild when the base table is under N rows" as a per-level runtime choice: measure N on ghcache and on `tests/shared_frontier_wide/wide_64`, keep whichever is cheaper per level, decided from `count(*)` in the probe (no new DDL). If the EXPLAIN shows an inner SCAN, that is a lower.pl index and belongs to `inner-scan-audit` (hail me; do not edit lower.pl).
4. COUNT test: `tests/one_tick_path.rs` gains a ghcache statements-per-fold cap at or below 7,113, and the per-verb `recount` cap.
5. Ledger entry; issue closed by the PR with the final table.

## Receipts to close
ghcache fold: statements <= 7,113 and wall <= 152 ms (the pre-#427 numbers), three runs; `tests/fixtures/ghcache_ticklog_base.txt` byte-identical; `grade.sh byte-clean=340`; idle tick still < 10.

## You own
`v6/sprefa-engine-rs/src/{incremental.rs,program.rs}`, `v6/sprefa-engine-rs/tests/{one_tick_path.rs,ordered_statement_count.rs}`, `docs/failure-modes.md`.
Forbidden: `sql.rs`, `driver.rs`, `run.rs` (lane fix-tick-transaction), `lower.pl`, `emit_rust.pl`, `emit_ts.pl`, `v6/dl/**`, conformance fixtures.

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
Batteries in the background with `timeout`; never foreground-wait more than 10 s. Commit per item; PUSH before you report.

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state only constraints the code cannot show. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). No new kernel; the frontier tables are the delta.

Done: `boop beep hail sprefa-coordinator --from one-path-busy-tick-cost --body "PR #<n>: statements/wall before->after, gate numbers"`; if refused, message the sprefa-* session over the cross-session socket. Blocked: one line, stop.
