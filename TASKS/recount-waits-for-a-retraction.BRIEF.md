# recount-waits-for-a-retraction

Issue: `issuectl show recount-waits-for-a-retraction` (design and a measured prototype are there; the lane that filed it kept a working tree at /tmp/shrink_gate_version.rs on this machine, read it if present). Base: `git merge --ff-only <sha the coordinator states>` first; fail = stop and hail. Branch `fix/recount-waits-for-a-retraction`. PR to main.

## The defect
`recount` (refcount reconcile, `incremental.rs::reconcile_ref_count_statement` and its two callers) re-derives `__support_next` from base tables for every head every time its clock fires: 5,630 of the ghcache fold's 9,884 statements after #430. In positive datalog a head only LOSES rows when a positive body rel loses rows or a negated body rel gains rows; additions never need a recount.

## Deliverable
1. Gate: recount a head only when, this tick, some positive body rel of it has a retraction (`_sign = -1` in its delta) or some negated body rel has an addition. The probe already reads per-rel EXISTS columns (#427 `TickWork::probe`); add the retraction column there, one chunked statement, no per-rel statement.
2. Prove the law before relying on it: a plunit or cargo test with (a) a positive-only rule, additions only across 5 ticks, recount count == 0 and final rows equal the oracle; (b) one retraction, recount fires once; (c) a negated body rel gaining a row, recount fires once. Tick logs byte-identical on all three against the naive (always-recount) run.
3. Receipts: `grade.sh byte-clean=340`, `tests/fixtures/ghcache_ticklog_base.txt` byte-identical, recount calls 5,630 -> (measured), per-verb table before/after, caps lowered in `tests/ordered_statement_count.rs`.
4. Ledger entry.

## You own
`v6/sprefa-engine-rs/src/incremental.rs` (recount gate and probe column only), `v6/sprefa-engine-rs/tests/{recount_gate.rs,ordered_statement_count.rs}`, `docs/failure-modes.md`.
Forbidden: `lower.pl` (lane delta-arm-subset-expansion), `sql.rs`, `driver.rs`, `run.rs` (lane fix-tick-transaction), `program.rs`, `v6/dl/**`, conformance fixtures. If the gate needs `program.rs`, hail with the line and stop.

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

## Measure, every step
```
cd v6 && swipl --stack_limit=12G -q -l prolog/compile.pl -l prolog/emit_rust.pl -g "compile_dl6('$PWD/dl/ghcache/ghcache.dl6','/tmp/gh.rs',[emitter(emit_rust:emit_program)])" -g halt
cargo build --release --manifest-path sprefa-engine-rs/Cargo.toml --bin emit_rust_harness
DL_ADAPTERS_DIR=$PWD/dl/ghcache DL_TRACE_SUMMARY=1 sprefa-engine-rs/target/release/emit_rust_harness /tmp/gh.rs dl/ghcache/ghcache.schedule.json --final 2>&1 >/dev/null | grep -A400 "DL_TRACE_SUMMARY =="
```
Three runs per arm; per-verb (us, calls) table before and after in the PR body. Baseline at your base sha: statements 11,534, wall ~235 ms. Target for the pair of arcs: 7,113 / 152 ms (pre-#427 ordered path).

## Style laws (CLAUDE.md)
No `eprintln!`; `tracing` only. Comments state only constraints the code cannot show. No em dashes. Banned words: provenance, substrate, load-bearing, regime, ground truth (say oracle), refusal, support (say refCount). `emit_ts.pl` output for unchanged programs stays byte-identical unless the shared predicate forces it; then say which files and why.

Done: `boop beep hail sprefa-coordinator --from <lane> --body "PR #<n>: numbers"`; if refused, message the sprefa-* session over the cross-session socket. Blocked: one line, stop.
