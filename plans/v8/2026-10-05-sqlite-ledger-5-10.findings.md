# Findings: ir:sqlite ledger rows 5-10 (settle-side work)

Brief: `plans/v8/2026-10-05-sqlite-ledger-5-10.brief.md`. Ledger:
`plans/v8/2026-10-05-sqlite-audit.findings.md`. sprefa branch `perf/sqlite-ledger-5-10` from main
`156fa84f3`; sqlite_ivm clone `sqlite_ivm/`, branch `perf/sqlite-ledger-5-10` from main `5d6e111`
(work counters merged; image cache off). Debug dl8, `-j 4`.

Proof is by deterministic counts (user rule of 2026-10-05): no wall-clock numbers below.

## Method

- Work counts: `ivm_sqlite::work` events (sqlite_ivm `0_work.rs`), `RUST_LOG=ivm_sqlite::work=info`.
  Registry: the pin `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts`, updated in the
  same commit pair as each fix. Census: the 129 `.dl7` under `fixtures/ examples/ book/src/probes/
  std/`, `dl8 compile`, sums over the 104 that exit 0 on both engines at main, less the four
  `fixtures/next/{0_graph_carry,1_counter,2_origin,6_wave_count}` (same set as the work-counts
  findings). Main column: executions 3,912,898 here vs 3,912,698 in the coordinator's baseline
  (programs that read git history see a different HEAD).
- Row attribution: a scratch `sqlite3_trace_v2` tracer (not committed, reverted) with a thread-local
  context set at `NodesPlan::run` (settle), `fixpoint`/`recompute` (loop), `Engine::settle` outside
  the plan (host), `intern_terms`/`intern_text` (term); counts per context and statement class
  (verb, written table kind, CTE or not), plus fixpoints and rounds. Built on the same six commits
  before the rebase onto `5d6e111` (settle SQL identical; install changes only). Census over all
  129 programs.
- Identity, every commit: census stdout+stderr+exit per program against main (129 programs);
  final: registry `cmp` both engines, macrotime oracle 24 cases x 2 engines.

## Rows on main (before), census, scratch tracer

| # | item | count on main |
|---|---|---|
| 5 | DRed loop statements prepared (bytes) | 196,361 (92,855,033); 41,478 stash/restore and 10,292 rederive statements prepared only for delete phases |
| 6 | settle fills carrying CTEs: executions / zero-row / VM steps | 64,040 / 49,164 / 693,665,660; 44,875 zero-row executions are fills whose CTE filters an input delta on `cX = K` |
| 7 | term interning: statements / VM steps / terms requested / texts requested | 1,614,674 / 47,909,198 / 490,978 / 81,414 (3 statements per new term) |
| 8 | DRed loop: executions / zero-row / VM steps; delete rounds; rows moved through hold | 447,566 / 328,634 / 195,622,898; 1,013; 562,362 |
| 9 | outside the plan: statements / VM steps; rows written to `frontier_P_delta`; delta read-back rows | 1,602,487 / 76,454,997; 514,991; 1,034,227 |
| 10 | integrate statements carrying CTEs: executions / prepared / VM steps | 9,454 / 8,428 / 67,861,733 |

All six rows were above the bar; each got one fix, in brief order of size (5, 6, 7, 8, 9, 10).

## Per row, census (scratch tracer; before -> after the row's commit)

| count | base | r5 | r6 | r7 | r8 | r9 | r10 |
|---|---|---|---|---|---|---|---|
| settle work: prepared | 308,534 | 254,622 | 254,995 | 260,880 | 238,366 | 236,988 | 232,803 |
| settle work: SQL bytes prepared | 225,081,422 | 216,483,430 | 216,508,421 | 220,572,415 | 216,679,506 | 216,461,122 | 214,733,250 |
| settle work: executions | 3,812,771 | 3,602,165 | 3,588,713 | 2,655,046 | 2,586,927 | 2,070,668 | 2,066,491 |
| settle work: zero-row executions | 914,870 | 728,137 | 714,102 | 714,100 | 660,663 | 660,809 | 655,980 |
| settle work: VM steps | 1,289,344,782 | 1,258,120,895 | 1,060,773,615 | 1,037,606,749 | 1,030,191,413 | 996,929,070 | 980,134,870 |
| R5 loop: prepared | 196,361 | 142,449 | 142,449 | 142,449 | 119,935 | 119,935 | 119,935 |
| R5 loop: SQL bytes prepared | 92,855,033 | 84,257,041 | 84,257,041 | 84,257,041 | 80,364,132 | 80,364,132 | 80,364,132 |
| R8 loop: executions | 447,566 | 236,960 | 236,960 | 236,960 | 168,841 | 168,841 | 168,841 |
| R8 loop: zero-row | 328,634 | 141,905 | 141,905 | 141,905 | 88,465 | 88,465 | 88,465 |
| R8 loop: VM steps | 195,622,898 | 164,399,011 | 164,399,011 | 164,399,011 | 156,983,675 | 156,983,675 | 156,983,675 |
| R8 fixpoints | 988 | 988 | 988 | 988 | 988 | 988 | 988 |
| R8 delete rounds | 1,013 | 36 | 36 | 36 | 36 | 36 | 36 |
| R8 insert rounds | 1,947 | 1,947 | 1,947 | 1,947 | 1,947 | 1,947 | 1,947 |
| R8 rows moved to/from hold | 562,362 | 708 | 708 | 708 | 708 | 708 | 708 |
| R6 settle fills +cte: executions | 64,040 | 64,040 | 50,014 | 50,014 | 50,014 | 50,014 | 50,014 |
| R6 settle fills +cte: zero-row | 49,164 | 49,164 | 35,138 | 35,138 | 35,138 | 35,138 | 35,138 |
| R6 settle fills +cte: prepared | 46,693 | 46,693 | 46,693 | 46,693 | 46,693 | 46,693 | 46,693 |
| R6 settle fills +cte: SQL bytes | 118,863,556 | 118,863,556 | 118,863,556 | 118,863,556 | 118,863,556 | 118,863,556 | 118,863,556 |
| R6 settle fills +cte: VM steps | 693,665,660 | 693,665,660 | 493,368,518 | 493,368,518 | 493,368,518 | 493,368,518 | 493,368,518 |
| R7 terms requested | 490,978 | 490,978 | 490,978 | 490,978 | 490,978 | 490,978 | 490,978 |
| R7 texts requested | 81,414 | 81,414 | 81,414 | 80,685 | 80,685 | 80,685 | 80,685 |
| R7 terms interned (ctor rows written) | 476,715 | 476,715 | 476,715 | 476,715 | 476,715 | 476,715 | 476,715 |
| R7 intern statements run | 1,614,674 | 1,614,674 | 1,614,674 | 681,007 | 681,007 | 681,007 | 681,007 |
| R7 intern VM steps | 47,909,198 | 47,909,198 | 47,909,198 | 24,742,332 | 24,742,332 | 24,742,332 | 24,742,332 |
| R9 host+stage statements | 1,602,487 | 1,602,487 | 1,602,487 | 1,602,487 | 1,602,487 | 1,086,228 | 1,086,228 |
| R9 host+stage VM steps | 76,454,997 | 76,454,997 | 76,454,997 | 76,454,997 | 76,454,997 | 43,192,654 | 43,192,654 |
| R9 rows into frontier_P_delta | 514,991 | 514,991 | 514,991 | 514,991 | 514,991 | 0 | 0 |
| R9 stage rows | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 |
| R9 host table rows | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 | 1,041,721 |
| R10 integrate +cte: executions | 9,454 | 9,454 | 9,454 | 9,454 | 9,454 | 9,454 | 4,727 |
| R10 integrate +cte: VM steps | 67,861,733 | 67,861,733 | 67,861,733 | 67,861,733 | 67,861,733 | 67,861,733 | 51,054,252 |
| R10 integrate +cte: prepared | 8,428 | 8,428 | 8,428 | 8,428 | 8,428 | 8,428 | 4,214 |
| R10 integrate +cte: SQL bytes | 3,925,420 | 3,925,420 | 3,925,420 | 3,925,420 | 3,925,420 | 3,925,420 | 2,194,041 |
| R10 by-key deletes | 5,695 | 5,695 | 5,695 | 5,695 | 5,695 | 5,695 | 6,245 |

The tracer's r7..r10 columns were measured with a session text cache in sprefa, since dropped
(no caching without a user request); its only effect was texts requested 81,414 -> 80,685. Row 7
without it, work counters: between executions 1,525,373 -> 665,157, VM steps 57,516,941 ->
36,177,553, prepared 14,454 -> 19,572, written_term unchanged.

Row 9 delta read-back (host `SELECT`): executions 3,668 -> 2,400, rows returned 1,034,227 -> 4,518,
VM steps 24,159,400 -> 19,200.

## Census, work counters (104 programs; main -> each commit)

c0: the counter fix alone (main code). base and c0 differ by 20 executions in programs that
read git history (run at different sprefa HEADs).

| count | base | c0 | r5 | r6 | r7 | r8 | r9 | r10 |
|---|---|---|---|---|---|---|---|---|
| creates | 361,173 | 361,173 | 361,173 | 361,173 | 361,173 | 361,173 | 361,173 | 361,173 |
| schema_rows_parsed | 25,351,585 | 25,351,585 | 25,351,585 | 25,351,585 | 25,351,585 | 25,351,585 | 25,351,585 | 25,351,585 |
| prepared | 664,347 | 664,347 | 619,119 | 619,434 | 624,552 | 605,840 | 604,659 | 601,072 |
| prepared_bytes | 305,629,662 | 305,629,662 | 298,420,670 | 298,441,775 | 302,043,019 | 298,804,290 | 298,617,641 | 297,130,004 |
| unfolded_bytes | 326,020,747 | 326,020,747 | 318,836,537 | 318,857,642 | 322,458,886 | 319,220,157 | 319,033,508 | 317,954,996 |
| executions | 3,912,898 | 3,912,918 | 3,735,206 | 3,722,928 | 2,862,712 | 2,804,867 | 2,327,686 | 2,324,131 |
| zero_row_executions | 839,183 | 839,186 | 681,646 | 668,877 | 668,877 | 623,485 | 623,599 | 619,523 |
| vm_steps | 1,233,021,682 | 1,233,025,976 | 1,204,462,532 | 1,009,487,221 | 988,147,833 | 981,641,442 | 950,947,785 | 935,470,287 |
| rows_returned | 3,505,837 | 3,505,863 | 3,505,315 | 3,958,603 | 3,527,634 | 3,515,181 | 2,563,023 | 2,999,311 |
| written_d | 4,981,118 | 4,981,128 | 4,461,598 | 4,461,598 | 4,461,598 | 4,461,598 | 4,461,598 | 4,461,598 |
| written_i | 2,483,847 | 2,483,854 | 2,483,854 | 2,483,854 | 2,483,854 | 2,483,854 | 2,483,854 | 2,483,854 |
| written_x | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| written_scratch | 3,184,308 | 3,184,317 | 2,664,787 | 2,664,787 | 2,664,787 | 2,664,787 | 2,000,009 | 2,000,009 |
| written_source | 616,359 | 616,363 | 616,363 | 616,363 | 616,363 | 616,363 | 616,363 | 616,363 |
| written_catalog | 3,489 | 3,489 | 3,489 | 3,489 | 3,489 | 3,489 | 3,489 | 3,489 |
| written_term | 533,670 | 1,016,390 | 1,016,390 | 1,016,390 | 1,016,390 | 1,016,390 | 1,016,390 | 1,016,390 |


Per phase: settle executions 1,992,715 -> 1,264,146, zero-row 333,847 -> 114,184, VM steps
1,164,455,657 -> 888,241,546, prepared 255,073 -> 186,680; between (term interning, snapshots)
executions 1,525,373 -> 665,157, VM steps 57,516,941 -> 36,177,553 (c0 -> r10).

`written_term`: the counter fix (sqlite_ivm `709a944`) attributes writes of statements reset
before completion (`INSERT ... RETURNING` read by `query_row`); main 533,670 -> 1,016,390 on the
same rows, and unchanged by row 7 (batched inserts write the same term rows). Registry:
16,012 constructor + 16,398 `ivm_term` + 386 `ivm_text` = 32,796 = pinned `between` written_term.
`prepared` rises at row 7 (+5,118): multi-row inserts of each chunk length are distinct texts.
`rows_returned` rises at row 6 (key reads) and row 10 (the upsert returns its touched rows).

## Registry build, work counters (pin totals over between + install + settle)

| count | main | c0 | r5 | r6 | r7 | r8 | r9 | r10 |
|---|---|---|---|---|---|---|---|---|
| creates | 4,963 | 4,963 | 4,963 | 4,963 | 4,963 | 4,963 | 4,963 | 4,963 |
| schema_rows_parsed | 620,094 | 620,094 | 620,094 | 620,094 | 620,094 | 620,094 | 620,094 | 620,094 |
| prepared | 11,723 | 11,723 | 11,350 | 11,353 | 11,438 | 11,278 | 11,261 | 11,213 |
| prepared_bytes | 5,565,848 | 5,565,848 | 5,502,658 | 5,502,859 | 5,564,535 | 5,534,880 | 5,532,137 | 5,502,919 |
| unfolded_bytes | 5,811,076 | 5,811,076 | 5,748,571 | 5,748,772 | 5,810,448 | 5,780,793 | 5,778,050 | 5,755,706 |
| executions | 135,311 | 135,311 | 132,276 | 131,574 | 99,864 | 98,017 | 83,874 | 83,894 |
| zero_row_executions | 25,061 | 25,061 | 22,384 | 21,673 | 21,673 | 20,263 | 20,265 | 20,185 |
| vm_steps | 57,101,727 | 57,101,727 | 56,044,140 | 44,888,589 | 44,106,994 | 43,787,850 | 42,833,211 | 42,216,224 |
| rows_returned | 115,551 | 115,551 | 115,543 | 134,189 | 118,285 | 117,848 | 89,606 | 105,687 |
| written_d | 240,214 | 240,214 | 221,502 | 221,502 | 221,502 | 221,502 | 221,502 | 221,502 |
| written_i | 108,610 | 108,610 | 108,610 | 108,610 | 108,610 | 108,610 | 108,610 | 108,610 |
| written_x | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| written_scratch | 136,901 | 136,901 | 118,189 | 118,189 | 118,189 | 118,189 | 95,433 | 95,433 |
| written_source | 23,381 | 23,381 | 23,381 | 23,381 | 23,381 | 23,381 | 23,381 | 23,381 |
| written_catalog | 39 | 39 | 39 | 39 | 39 | 39 | 39 | 39 |
| written_term | 22,565 | 38,964 | 38,964 | 38,964 | 38,964 | 38,964 | 38,964 | 38,964 |

## How it works

- Row 5 (sqlite_ivm `b1946b4`). DRed retractions enter a LetRec only through delete seeds (an
  antijoin key appearing) or negative rows in a copy delta. `fixpoint` runs the delete seeds
  first (they read no copy), then probes each active copy with `SELECT EXISTS (... WHERE w < 0)`.
  With neither, stash, delete rounds, rederive and restore are skipped: the delete phase would
  read only empty deltas. Each skipped fixpoint saves the positive rows' trip to `_hold` and back.
- Row 6 (`6e28cd5`). An inline Mfp whose filter holds `cX = K` (plain integer equality) over a
  stored delta is empty unless that delta has a row with `cX = K`. At compile time each settle
  fill gets the conditions under which its inputs can be nonempty (one ascending pass over the
  plan nodes: stored delta -> nonempty; eq-Mfp over a stored delta -> key test; other inline
  node -> any input). Where four or more fills test keys of one `(delta, column)`, the settle
  reads `SELECT DISTINCT cX FROM delta` once (dropped when the delta is written again) and runs a
  fill only when one condition holds.
- Row 7 (`7aee47f`). `intern_terms` looks each term up once (SQL formatted once per functor),
  assigns new ids in Rust from one read of `max(ivm_term.id)`, and writes `ivm_term` and each
  constructor table with multi-row inserts (256 rows). A term repeated in a batch keeps its first
  id. No cache across calls; sprefa unchanged.
- Row 8 (`061db50`). A variable's phase statement reads its body delta and is skipped when that
  delta is inactive. `to_delta`/`to_acc`/`to_del`/`clear_nx` run only for a variable that staged
  rows in `nx`; the variable delta was cleared earlier in the round, so `to_delta`'s row count is
  its change and the per-round `SELECT EXISTS` is gone.
- Row 9 (`c903c6a`). `Engine::settle` used `settle_counted`, which wrote the plan's output rows to
  `frontier_P_delta` one statement per row, read them back as cells, discarded those, and read
  the table again for weights. `settle_rows` returns `NodesPlan::run`'s rows (a threshold output
  keeps the sign) and bumps the frontier. Source changes are staged as `Staged` rows borrowing
  the relation name and cells.
- Row 10 (`d36543e`). An integrated node whose delta is a CTE chain ran the chain in the upsert
  and again in `DELETE ... IN (SELECT .. FROM <delta>)`. The upsert now ends `RETURNING w, c0..`;
  rows left at weight 0 are deleted by primary key.

## Validation

- Census identity at every commit: 129 programs, exit codes, stdout, stderr (less phase/ir lines)
  identical to main (ir:sqlite); sprefa source unchanged (pins and plans only).
- Registry `cmp` with `hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs`: identical, both
  engines; build output dirs identical to main.
- Macrotime oracle: `dl8 expand` on the 24 `oracle/macrotime/*_<n>.json`, both engines, main vs
  final: 48 pairs, 0 differences.
- sprefa `cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book --test-threads=1`:
  ir:sqlite 174 passed 0 failed; ir:dd 174 passed 0 failed (extension dylib rebuilt from the clone).
- sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1`: 254 passed after
  the span pin fixes (one failure before them, `11_settle_spans`, fixed in rows 9 and 10).
- Pins moved, each in its row's commit: `tests/it/_9h_build.rs` registry work (every row);
  `13_work_counts` empty settles -2 statements (row 9); `11_settle_spans` statement and round
  counts (rows 5, 8, 9, 10). No golden or snapshot file changed.

## Overlap with lab/sqlite-install

- Row 6's gate also removes first prepares of gated fills (ledger row 1) where a fill is skipped
  in every settle of an install; the census shows none (prepared fills unchanged at 46,693).
- `catalog.rs` `SettleSql::weight_delta` is no longer read (dead-field warning); left in place
  because `catalog.rs` is the lab's file.
