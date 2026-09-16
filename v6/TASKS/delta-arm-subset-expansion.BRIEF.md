# delta-arm-subset-expansion

Issue: `issuectl show delta-arm-subset-expansion` (the measurement table and the emitted SQL sizes are there). Base: `git merge --ff-only c49021157b1912b6ad2e75fff33320589de3d8f3` first; fail = stop and hail. Branch `fix/delta-arm-subset-expansion`. PR to main.

## The defect
`lower.pl`'s delta plan emits 2^N `UNION ALL` arms for a level with N body items (every subset of "which items moved"). `page_response` has 256 arms in a 248 KB statement, 3.7 ms a call against 0.67 ms for a rebuild. Incremental view maintenance needs N arms: arm i = item i's frontier joined against the OTHER items' state, where items before i read their NEW state (base after promote, or base + frontier) and items after i read their OLD state (base before promote). That is the standard delta-of-a-join identity (DBSP's bilinear rule, N terms); the 2^N form is its fully expanded version and is correct but exponential.

## Deliverable
1. Read first: the predicate in `lower.pl` that enumerates the subsets (grep for the `UNION ALL` arm builder behind `levels[i].insert_sql`; the issue names it), `incremental.rs` to learn what the runtime guarantees about base vs frontier at the moment a level runs (which tables hold old state, which hold old + new), and `v6/prolog/conformance/rulings.pl` for anything on delta shapes.
2. Design in the PR body before code: the N-arm identity written out for N=3 with the exact tables each arm reads (old/new per item), and why the runtime's promote order makes those tables available. Planning protocol: signatures, pseudo-code, lifetimes, storage then read/write order.
3. Implement: N arms. Negated items and aggregates keep whatever arm shape they have today unless the identity covers them; say which.
   REQUIRED addition (found by lane fix-recount-waits-for-a-retraction): a head whose body has `not(R)` gains rows when R SHRINKS, and no delta insert arm exists for that today; the recount pass is the only producer. Pinned case: fixture `callgraph_unused_inverts_with_the_call_set` tick 4, `-call('b.rs',main)` must add `unused(main)`. Emit that arm (R's departure frontier joined against the positive items' state, filtered by `not exists` on R's post-promote base). Receipt: that fixture byte-clean with `DL_NO_SHRINK_GATE` unset once the recount lane's gate merges; coordinate the order with the coordinator, do not edit incremental.rs.
4. Receipts: `grade.sh byte-clean=340` (the corpus-wide oracle agreement is THE correctness receipt for a delta rewrite), `tests/fixtures/ghcache_ticklog_base.txt` byte-identical, `insert_sql` size for `page_response` 248 KB -> under 10 KB, arms 256 -> 8, a plunit test pinning arm count == N on an inline 4-item rule, per-verb table before/after, `level_insert` us per call before/after.
5. `emit_ts.pl` shares `lower.pl`: diff the whole committed TS corpus (`v6/prolog/compile/out/*.ts`) and regenerate what moves; list every moved file in the PR body.
6. Ledger entry; ARCH row if one names this.

## You own
`v6/prolog/lower.pl` (delta plan predicates), `v6/prolog/compile/test/plunit_tests.pl` (additive), `v6/prolog/compile/out/*.ts` (regeneration only), `v6/sprefa-engine-rs/tests/one_tick_path.rs` (caps), `docs/failure-modes.md`, `v6/prolog/ARCH.pl` (one row).
Forbidden: `incremental.rs`, `program.rs`, `sql.rs`, `driver.rs`, `run.rs` (lanes fix-tick-transaction and recount-waits-for-a-retraction own the runtime), `emit_rust.pl`, `v6/dl/**`, conformance fixtures.

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
