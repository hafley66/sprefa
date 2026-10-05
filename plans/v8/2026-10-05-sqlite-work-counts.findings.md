# Findings: sqlite work counts per sqlite_ivm commit

Brief: `plans/v8/2026-10-05-sqlite-work-counts.brief.md`. sprefa `feat/sqlite-work-counts` from
`cd9c68d41`; sqlite_ivm clone `sqlite_ivm/`, branch `feat/sqlite-work-counts` from main `63c1123`
(`4710596` plus the `image` feature off by default). Debug dl8 (`[profile.dev] opt-level = 1`,
dependencies `opt-level = 3`), `-j 4`. No clock anywhere: every number below is a count, and
every count was identical across two runs of the same binary (registry, both engines).

## Counter

`crates/ivm-sqlite/src/0_work.rs` (sqlite_ivm commit `04ff67f`). `Sqlite::install_counted`, or
`Engine::install` while the `ivm_sqlite::work` target is enabled at info, registers
`sqlite3_trace_v2(STMT | PROFILE | ROW)` on the connection. One `ivm_sqlite::work` info event per
install, per settle, and `between` for the work outside both (term interning, snapshots, text
reads). Off: no trace registered, no behavior change; on: `13_work_counts` asserts the same
settle statements as an uncounted install.

| count | source |
|---|---|
| creates | executions whose statement is a `CREATE` |
| schema_rows_parsed | per CREATE, rows of the target schema's `sqlite_master` after it: the full scan `OP_ParseSchema` runs (`SELECT*FROM"<schema>".sqlite_master WHERE tbl_name=..`). The reparse runs with `db->init.busy` (no SQL text, no STMT/PROFILE event); its result rows arrive as ROW events on a text-less statement and grow the schema's running row count |
| prepared, prepared_bytes | statement objects at their first run (`SQLITE_STMTSTATUS_RUN` = 0 at `OP_Init`), and their `sqlite3_sql` bytes; covers rusqlite, `execute_batch` and `sqlite3_exec` |
| unfolded_bytes | the prepared text with every FROM-position CTE reference replaced by the CTE body, recursively, `WITH` headers dropped (qualifier uses `x.c` are not references). Checked against an independent regex implementation on the 18 largest registry statements of `1074daa`: equal on all 18 |
| executions | PROFILE events (statement completed or reset after starting) |
| zero_row_executions | reads and writes (`SELECT`/`VALUES`/`INSERT`/`DELETE`/`UPDATE`/`REPLACE`, after any `WITH`) that returned no row and wrote no row |
| vm_steps, fullscan_steps, sorts, autoindexes, reprepares | `sqlite3_stmt_status` per execution, reset on read |
| rows_returned | ROW events |
| written_<kind> | `sqlite3_total_changes` over the execution minus nested statements' writes, by the written table: `_d` / `_i` / `_x` (`*_n<k>_d`, `_i`, `_x<j>`), scratch (other `*_n<k>_*`: `_nx`, `_acc`, `_hold`, `_g`, ..., and `frontier_<p>_stage/_delta/...`), source (host tables), catalog (`frontier_catalog*`, `frontier_dependency`, `frontier_install`), term (`ivm_term`, `ivm_text`, `ivm_ctor_*`, dictionaries) |

Limits: a statement prepared and never stepped is not counted as prepared; statements nested two
deep in SQL functions subtract their writes from the outermost only once.

dd: the existing `ivm_dd` debug events (`dd install`: operators built, nodes, arrangements;
`dd settle`: changes in/out, epochs, worker steps) plus `arranged_records` (record updates sent
into join arrangements during the settle; added in the same commit).

Backports: the same `0_work.rs` and the same five hooks in `4_engine.rs` applied in the clone's
working tree at `1074daa`, `a9e4b37`, `4710596` (that commit with `default = []` for
`ivm-sqlite`, so its image cache is off), built into dl8, reverted; no commit on any old branch.
sprefa `cd9c68d41` uses `ErrorKind::LetRecLimit` (added after `a9e4b37`); for all three old
builds `engine_diagnostic`'s `LetRecLimit` arm was removed in the working tree and restored after.
The four programs that need LetRec limits (`fixtures/next/{0_graph_carry,1_counter,2_origin,6_wave_count}`)
exit 1 on `1074daa` and `a9e4b37`.

Runs: registry `dl8 build --out <dir> std/tsi_registry_emit.dl7`, `cmp` identical to
`hafley-rs/crates/hafley_scm/src/read/tsi/registry.rs` at all four commits (sqlite) and on dd.
Census: every `.dl7` under `fixtures/ examples/ book/src/probes/ std/` (129), `dl8 compile`,
`RUST_LOG=ivm_sqlite::work=info,ivm_dd=debug`; sums over the 104 that exit 0 at all four commits
and on dd (315 installs, 506 settles per engine, as in the audit).

### registry

| count | 1074daa | a9e4b37 | 4710596 | main | main / 1074daa | dd |
|---|---|---|---|---|---|---|
| total creates | 4,624 | 4,963 | 4,963 | 4,963 | 1.073 | |
| total schema_rows_parsed | 4,655,563 | 5,487,631 | 620,094 | 620,094 | 0.133 | |
| total prepared | 10,946 | 11,699 | 11,723 | 11,723 | 1.071 | |
| total prepared_bytes | 6,893,232 | 5,532,609 | 5,565,848 | 5,565,848 | 0.807 | |
| total unfolded_bytes | 11,392,425 | 5,777,837 | 5,811,076 | 5,811,076 | 0.510 | |
| total executions | 134,275 | 135,287 | 135,311 | 135,311 | 1.008 | |
| total zero_row_executions | 24,852 | 25,061 | 25,061 | 25,061 | 1.008 | |
| total vm_steps | 66,443,585 | 57,077,916 | 57,101,727 | 57,101,727 | 0.859 | |
| total fullscan_steps | 4,830,160 | 4,712,566 | 4,712,566 | 4,712,566 | 0.976 | |
| total sorts | 29,180 | 19,491 | 19,491 | 19,491 | 0.668 | |
| total autoindexes | 23,363 | 23,363 | 23,363 | 23,363 | 1.000 | |
| total reprepares | 3 | 3 | 3 | 3 | 1.000 | |
| total rows_returned | 115,455 | 115,551 | 115,551 | 115,551 | 1.001 | |
| total written_d | 179,470 | 240,214 | 240,214 | 240,214 | 1.338 | |
| total written_i | 108,610 | 108,610 | 108,610 | 108,610 | 1.000 | |
| total written_x | 0 | 0 | 0 | 0 | - | |
| total written_scratch | 136,901 | 136,901 | 136,901 | 136,901 | 1.000 | |
| total written_source | 23,381 | 23,381 | 23,381 | 23,381 | 1.000 | |
| total written_catalog | 39 | 39 | 39 | 39 | 1.000 | |
| total written_term | 22,565 | 22,565 | 22,565 | 22,565 | 1.000 | |
| install creates | 4,624 | 4,963 | 4,963 | 4,963 | 1.073 | |
| install schema_rows_parsed | 4,655,563 | 5,487,631 | 620,094 | 620,094 | 0.133 | |
| install prepared | 4,927 | 5,266 | 5,290 | 5,290 | 1.074 | |
| install prepared_bytes | 1,577,185 | 1,664,285 | 1,697,524 | 1,697,524 | 1.076 | |
| install unfolded_bytes | 1,577,185 | 1,664,285 | 1,697,524 | 1,697,524 | 1.076 | |
| install executions | 4,927 | 5,266 | 5,290 | 5,290 | 1.074 | |
| install zero_row_executions | 58 | 58 | 58 | 58 | 1.000 | |
| install vm_steps | 138,751 | 148,243 | 148,531 | 148,531 | 1.070 | |
| install fullscan_steps | 138 | 138 | 138 | 138 | 1.000 | |
| install sorts | 358 | 358 | 358 | 358 | 1.000 | |
| install rows_returned | 170 | 170 | 170 | 170 | 1.000 | |
| install written_catalog | 28 | 28 | 28 | 28 | 1.000 | |
| install written_term | 137 | 137 | 137 | 137 | 1.000 | |
| settle prepared | 5,875 | 6,289 | 6,289 | 6,289 | 1.070 | |
| settle prepared_bytes | 5,307,196 | 3,859,473 | 3,859,473 | 3,859,473 | 0.727 | |
| settle unfolded_bytes | 9,806,389 | 4,104,701 | 4,104,701 | 4,104,701 | 0.419 | |
| settle executions | 76,815 | 77,488 | 77,488 | 77,488 | 1.009 | |
| settle zero_row_executions | 8,076 | 8,285 | 8,285 | 8,285 | 1.026 | |
| settle vm_steps | 64,415,029 | 55,039,868 | 55,063,391 | 55,063,391 | 0.855 | |
| settle fullscan_steps | 4,804,684 | 4,687,090 | 4,687,090 | 4,687,090 | 0.976 | |
| settle sorts | 28,822 | 19,133 | 19,133 | 19,133 | 0.664 | |
| settle autoindexes | 23,363 | 23,363 | 23,363 | 23,363 | 1.000 | |
| settle rows_returned | 51,782 | 51,878 | 51,878 | 51,878 | 1.002 | |
| settle written_d | 179,470 | 240,214 | 240,214 | 240,214 | 1.338 | |
| settle written_i | 108,610 | 108,610 | 108,610 | 108,610 | 1.000 | |
| settle written_scratch | 136,901 | 136,901 | 136,901 | 136,901 | 1.000 | |
| settle written_source | 23,381 | 23,381 | 23,381 | 23,381 | 1.000 | |
| settle written_catalog | 11 | 11 | 11 | 11 | 1.000 | |
| settle written_term | 6,030 | 6,030 | 6,030 | 6,030 | 1.000 | |
| between prepared | 144 | 144 | 144 | 144 | 1.000 | |
| between prepared_bytes | 8,851 | 8,851 | 8,851 | 8,851 | 1.000 | |
| between unfolded_bytes | 8,851 | 8,851 | 8,851 | 8,851 | 1.000 | |
| between executions | 52,533 | 52,533 | 52,533 | 52,533 | 1.000 | |
| between zero_row_executions | 16,718 | 16,718 | 16,718 | 16,718 | 1.000 | |
| between vm_steps | 1,889,805 | 1,889,805 | 1,889,805 | 1,889,805 | 1.000 | |
| between fullscan_steps | 25,338 | 25,338 | 25,338 | 25,338 | 1.000 | |
| between reprepares | 3 | 3 | 3 | 3 | 1.000 | |
| between rows_returned | 63,503 | 63,503 | 63,503 | 63,503 | 1.000 | |
| between written_term | 16,398 | 16,398 | 16,398 | 16,398 | 1.000 | |
| dd install events | | | | | | 3 |
| dd install operators | | | | | | 21,603 |
| dd install nodes | | | | | | 5,766 |
| dd install arrangements | | | | | | 1,474 |
| dd settle events | | | | | | 9 |
| dd settle changes_in | | | | | | 37,225 |
| dd settle changes_out | | | | | | 18,997 |
| dd settle epochs | | | | | | 9 |
| dd settle steps | | | | | | 949 |
| dd settle arranged_records | | | | | | 115,730 |

### census

| count | 1074daa | a9e4b37 | 4710596 | main | main / 1074daa | dd |
|---|---|---|---|---|---|---|
| total creates | 336,597 | 361,173 | 361,173 | 361,173 | 1.073 | |
| total schema_rows_parsed | 195,166,199 | 227,020,399 | 25,351,585 | 25,351,585 | 0.130 | |
| total prepared | 618,043 | 661,827 | 664,347 | 664,347 | 1.075 | |
| total prepared_bytes | 398,998,671 | 303,245,735 | 305,629,662 | 305,629,662 | 0.766 | |
| total unfolded_bytes | 796,755,638 | 323,636,820 | 326,020,747 | 326,020,747 | 0.409 | |
| total executions | 3,862,562 | 3,910,178 | 3,912,698 | 3,912,698 | 1.013 | |
| total zero_row_executions | 827,743 | 839,153 | 839,153 | 839,153 | 1.014 | |
| total vm_steps | 1,519,568,819 | 1,232,232,034 | 1,232,977,186 | 1,232,977,186 | 0.811 | |
| total fullscan_steps | 100,672,303 | 96,935,849 | 96,935,849 | 96,935,849 | 0.963 | |
| total sorts | 1,068,886 | 608,641 | 608,641 | 608,641 | 0.569 | |
| total autoindexes | 338,828 | 339,631 | 339,631 | 339,631 | 1.002 | |
| total reprepares | 24 | 24 | 24 | 24 | 1.000 | |
| total rows_returned | 3,503,479 | 3,505,577 | 3,505,577 | 3,505,577 | 1.001 | |
| total written_d | 3,515,416 | 4,981,016 | 4,981,016 | 4,981,016 | 1.417 | |
| total written_i | 2,483,777 | 2,483,777 | 2,483,777 | 2,483,777 | 1.000 | |
| total written_x | 0 | 0 | 0 | 0 | - | |
| total written_scratch | 3,184,218 | 3,184,218 | 3,184,218 | 3,184,218 | 1.000 | |
| total written_source | 616,319 | 616,319 | 616,319 | 616,319 | 1.000 | |
| total written_catalog | 3,489 | 3,489 | 3,489 | 3,489 | 1.000 | |
| total written_term | 533,640 | 533,640 | 533,640 | 533,640 | 1.000 | |
| install creates | 336,597 | 361,173 | 361,173 | 361,173 | 1.073 | |
| install schema_rows_parsed | 195,166,199 | 227,020,399 | 25,351,585 | 25,351,585 | 0.130 | |
| install prepared | 367,724 | 392,300 | 394,820 | 394,820 | 1.074 | |
| install prepared_bytes | 101,787,940 | 107,522,522 | 109,906,449 | 109,906,449 | 1.080 | |
| install unfolded_bytes | 101,787,940 | 107,522,522 | 109,906,449 | 109,906,449 | 1.080 | |
| install executions | 367,724 | 392,300 | 394,820 | 394,820 | 1.074 | |
| install zero_row_executions | 5,917 | 5,917 | 5,917 | 5,917 | 1.000 | |
| install vm_steps | 10,331,132 | 11,019,260 | 11,049,500 | 11,049,500 | 1.070 | |
| install fullscan_steps | 14,338 | 14,338 | 14,338 | 14,338 | 1.000 | |
| install sorts | 32,702 | 32,702 | 32,702 | 32,702 | 1.000 | |
| install rows_returned | 17,502 | 17,502 | 17,502 | 17,502 | 1.000 | |
| install written_catalog | 2,938 | 2,938 | 2,938 | 2,938 | 1.000 | |
| install written_term | 14,144 | 14,144 | 14,144 | 14,144 | 1.000 | |
| settle prepared | 235,865 | 255,073 | 255,073 | 255,073 | 1.081 | |
| settle prepared_bytes | 296,312,836 | 194,825,318 | 194,825,318 | 194,825,318 | 0.657 | |
| settle unfolded_bytes | 694,069,803 | 215,216,403 | 215,216,403 | 215,216,403 | 0.310 | |
| settle executions | 1,969,575 | 1,992,615 | 1,992,615 | 1,992,615 | 1.012 | |
| settle zero_row_executions | 322,437 | 333,847 | 333,847 | 333,847 | 1.035 | |
| settle vm_steps | 1,451,725,322 | 1,163,700,409 | 1,164,415,321 | 1,164,415,321 | 0.802 | |
| settle fullscan_steps | 99,968,161 | 96,231,707 | 96,231,707 | 96,231,707 | 0.963 | |
| settle sorts | 1,036,184 | 575,939 | 575,939 | 575,939 | 0.556 | |
| settle autoindexes | 338,828 | 339,631 | 339,631 | 339,631 | 1.002 | |
| settle rows_returned | 1,503,756 | 1,505,854 | 1,505,854 | 1,505,854 | 1.001 | |
| settle written_d | 3,515,416 | 4,981,016 | 4,981,016 | 4,981,016 | 1.417 | |
| settle written_i | 2,483,777 | 2,483,777 | 2,483,777 | 2,483,777 | 1.000 | |
| settle written_scratch | 3,184,218 | 3,184,218 | 3,184,218 | 3,184,218 | 1.000 | |
| settle written_source | 616,319 | 616,319 | 616,319 | 616,319 | 1.000 | |
| settle written_catalog | 551 | 551 | 551 | 551 | 1.000 | |
| settle written_term | 36,820 | 36,820 | 36,820 | 36,820 | 1.000 | |
| between prepared | 14,454 | 14,454 | 14,454 | 14,454 | 1.000 | |
| between prepared_bytes | 897,895 | 897,895 | 897,895 | 897,895 | 1.000 | |
| between unfolded_bytes | 897,895 | 897,895 | 897,895 | 897,895 | 1.000 | |
| between executions | 1,525,263 | 1,525,263 | 1,525,263 | 1,525,263 | 1.000 | |
| between zero_row_executions | 499,389 | 499,389 | 499,389 | 499,389 | 1.000 | |
| between vm_steps | 57,512,365 | 57,512,365 | 57,512,365 | 57,512,365 | 1.000 | |
| between fullscan_steps | 689,804 | 689,804 | 689,804 | 689,804 | 1.000 | |
| between reprepares | 24 | 24 | 24 | 24 | 1.000 | |
| between rows_returned | 1,982,221 | 1,982,221 | 1,982,221 | 1,982,221 | 1.000 | |
| between written_term | 482,676 | 482,676 | 482,676 | 482,676 | 1.000 | |
| dd install events | | | | | | 315 |
| dd install operators | | | | | | 1,598,495 |
| dd install nodes | | | | | | 460,233 |
| dd install arrangements | | | | | | 111,198 |
| dd settle events | | | | | | 506 |
| dd settle changes_in | | | | | | 967,893 |
| dd settle changes_out | | | | | | 500,417 |
| dd settle epochs | | | | | | 506 |
| dd settle steps | | | | | | 11,890 |
| dd settle arranged_records | | | | | | 2,042,363 |


`main` = `63c1123` + counter; its counts equal `4710596` in every row (the commit only changes
the `image` default, off in both columns).

## sqlite / dd

| ratio (main) | registry | census |
|---|---|---|
| statements prepared / dd operators built | 11,723 / 21,603 = 0.54 | 664,347 / 1,598,495 = 0.42 |
| settle rows written (all kinds) / dd arranged records | 515,147 / 115,730 = 4.45 | 11,302,701 / 2,042,363 = 5.53 |
| all rows written / dd arranged records | 531,710 / 115,730 = 4.59 | 11,802,459 / 2,042,363 = 5.78 |
| settle written `_d` / dd arranged records | 240,214 / 115,730 = 2.08 | 4,981,016 / 2,042,363 = 2.44 |
| CREATEs / dd arrangements | 4,963 / 1,474 = 3.37 | 361,173 / 111,198 = 3.25 |

VM steps, executions and schema rows have no dd counterpart.

## Per fix

### `a9e4b37` (CTE expansion cap + stored multi-read unions), against `1074daa`

| count | registry | census |
|---|---|---|
| settle unfolded bytes | 9,806,389 -> 4,104,701 (-58.1 %) | 694,069,803 -> 215,216,403 (-69.0 %) |
| settle prepared bytes | 5,307,196 -> 3,859,473 (-27.3 %) | 296,312,836 -> 194,825,318 (-34.3 %) |
| settle VM steps | 64,415,029 -> 55,039,868 (-14.6 %) | 1,451,725,322 -> 1,163,700,409 (-19.8 %) |
| settle sorts | 28,822 -> 19,133 (-33.6 %) | 1,036,184 -> 575,939 (-44.4 %) |
| settle full-scan steps | 4,804,684 -> 4,687,090 (-2.4 %) | 99,968,161 -> 96,231,707 (-3.7 %) |
| up: CREATEs | 4,624 -> 4,963 (+339, +7.3 %) | 336,597 -> 361,173 (+24,576, +7.3 %) |
| up: schema rows parsed | 4,655,563 -> 5,487,631 (+17.9 %) | 195,166,199 -> 227,020,399 (+16.3 %) |
| up: statements prepared | 10,946 -> 11,699 (+753) | 618,043 -> 661,827 (+43,784, +7.1 %) |
| up: install prepared bytes | 1,577,185 -> 1,664,285 (+5.5 %) | 101,787,940 -> 107,522,522 (+5.6 %) |
| up: executions | 134,275 -> 135,287 (+1,012) | 3,862,562 -> 3,910,178 (+47,616, +1.2 %) |
| up: zero-row executions | 24,852 -> 25,061 (+209) | 827,743 -> 839,153 (+11,410, +1.4 %) |
| up: `_d` rows written | 179,470 -> 240,214 (+33.8 %) | 3,515,416 -> 4,981,016 (+41.7 %) |
| up: automatic indexes | equal (23,363) | 338,828 -> 339,631 (+803) |
| up: rows returned | 115,455 -> 115,551 (+96) | 3,503,479 -> 3,505,577 (+2,098) |

The stored unions are the extra `_d` tables: more CREATEs (and so more schema rows per CREATE),
more fill statements and executions, more `_d` rows written; in exchange each settle statement
carries less CTE text and SQLite expands less of it.

### `4710596` (sqlite3_exec DDL, 8 shard schemas; image cache off), against `a9e4b37`

| count | registry | census |
|---|---|---|
| schema rows parsed | 5,487,631 -> 620,094 (-88.7 %) | 227,020,399 -> 25,351,585 (-88.8 %) |
| CREATEs | equal | equal |
| up: statements prepared, executions | +24 (3 installs x 8 `ATTACH`) | +2,520 (315 x 8) |
| up: install prepared bytes | +33,239 | +2,383,927 (+2.2 %) |
| up: install VM steps | +288 | +30,240 |
| up: settle VM steps | +23,523 (+0.04 %) | +714,912 (+0.06 %) |

Settle statement texts are unchanged by this commit (lab `plan_text_dump`); the settle VM-step
increase is not explained here (one hypothesis, unverified: one more `OP_Transaction` per extra
schema a statement touches).

### Whole range, `1074daa` -> main

Census: schema rows parsed -87.0 %, settle unfolded bytes -69.0 %, prepared bytes -23.4 %, VM
steps -18.9 %, sorts -43.1 %. Went up over the range: CREATEs +7.3 %, statements prepared
+7.5 %, executions +1.3 %, zero-row executions +1.4 %, `_d` rows written +41.7 %, automatic
indexes +0.2 %, rows returned +0.06 %. Unchanged: `_i`, scratch, source, catalog and term rows
written, `between` (interning, snapshots) in every row, reprepares.

## Validation

- Registry `cmp`: identical at `1074daa`, `a9e4b37`, `4710596`, main (sqlite) and main (dd).
- Determinism: registry counts equal across two runs of the same binary, sqlite and dd.
- `ivm-sqlite` `13_work_counts` (new; image off): pins install + empty settle counts of the
  registry's three IRs and c15, and asserts counting leaves the settle statements unchanged.
- sprefa `tests/it/_9h_build.rs::tsi_registry_sqlite_work_counts` (new): pins the registry
  build's per-phase sums.
- sqlite_ivm `cargo test -j 4 --workspace --no-fail-fast -- --test-threads=1`: 254 passed, 0 failed
  (`13_work_counts` compiled out there: the workspace's `bundled` feature turns `image` on; run
  alone with `-p ivm-sqlite`: passed).
- sprefa `DL8_ENGINE=<e> cargo test -j 4 --workspace --no-fail-fast -- --skip _7_book
  --test-threads=1`, extension dylib rebuilt from the clone: ir:sqlite 174 passed 0 failed;
  ir:dd 174 passed 0 failed.

## Overlap

`4_engine.rs` (`Sqlite` gains a `work` field first; `install_on_with` delegates to
`install_traced`; `Engine::install` starts the trace; three `report` lines in `settle`) and
`ivm-dd/src/2_dd.rs` (`ArrangeStats::records`, `dd settle` field). `perf/sqlite-ledger-5-10`
edits settle-side code in `4_engine.rs`; the hooks sit at the head of `settle` and in its two
final match arms.
