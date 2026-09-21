# LAB tick per-verb remeasure on main

Base `9e4b468157bb2a189960b8ec69daad10af372862`. Release build. Medians of 3, arms interleaved run by run (ghcache, wide_64, ghcache, wide_64, ghcache, wide_64). No `src/**` change.

- [Commands](#commands)
- [ghcache, 14-tick fold](#ghcache-14-tick-fold)
- [wide_64, 3-tick fold](#wide_64-3-tick-fold)
- [Per-verb table](#per-verb-table)
- [Rust remainder per tick](#rust-remainder-per-tick)
- [Run logs](#run-logs)

## Commands

```bash
cd v6/sprefa-engine-rs
cargo build --release --bin emit_rust_harness

# compile each program once (swipl, 12G stack)
swipl --stack_limit=12G -q -l ../prolog/compile.pl -l ../prolog/emit_rust.pl \
  -g "compile_dl6('../dl/ghcache/ghcache.dl6','/tmp/ghcache.rs',[emitter(emit_rust:emit_program)])" -g halt
swipl --stack_limit=12G -q -l ../prolog/compile.pl -l ../prolog/emit_rust.pl \
  -g "compile_dl6('tests/shared_frontier_wide/wide_64.dl6','/tmp/wide_64.rs',[emitter(emit_rust:emit_program)])" -g halt

# fold, info level (all timing cells)
DL_ADAPTERS_DIR="$(pwd)/../dl/ghcache" RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 target/release/emit_rust_harness /tmp/ghcache.rs \
  "$(pwd)/../dl/ghcache/ghcache.schedule.json" --final

RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 target/release/emit_rust_harness /tmp/wide_64.rs \
  "$(pwd)/tests/shared_frontier_wide/wide_64.schedule.json" --final

# fold, trace level (per-tick sqlite, one run per corpus)
#   same two commands with RUST_LOG=sprefa_engine_rs=trace
```

Cell sources:

| cell | source |
| --- | --- |
| fold statements | `SEAM_TALLY.statements` (seam tally event) minus `ddl`/`boot`/`unlabelled` calls |
| per-verb `us/calls` | `DL_TRACE_SUMMARY` block, verb column summed across relations |
| tick-only SQLite us | `TOTAL sqlite` minus `ddl`+`boot`+`unlabelled` sqlite |
| fold wall ms | sum of per-tick driver span `time.busy` (info level) |
| per-tick sqlite us | trace run, per-statement `prepare_us`+`step_us` summed per tick |

## ghcache, 14-tick fold

Medians of 3. `--release`, `RUST_LOG=info`.

| cell | value |
| --- | --- |
| fold statements | 6967 |
| publish calls | 326 |
| publish us | 9379 |
| tick-only SQLite us | 83394 |
| fold wall ms | 100.84 |
| rust remainder us (wall - sqlite) | 17446 |

## wide_64, 3-tick fold

Medians of 3. `--release`, `RUST_LOG=info`.

| cell | value |
| --- | --- |
| fold statements | 2185 |
| publish calls | 384 |
| publish us | 5426 |
| tick-only SQLite us | 24030 |
| fold wall ms | 32.87 |
| rust remainder us (wall - sqlite) | 8840 |

## Per-verb table

Medians of 3, `us/calls` (sqlite us). `-` means the verb does not fire on that corpus.

| verb | ghcache | wide_64 |
| --- | --- | --- |
| level_insert | 30957/1202 | 3041/192 |
| recount | 18304/3358 | - |
| publish | 9379/326 | 5426/384 |
| probe | 8870/28 | 2037/3 |
| stage | 7051/558 | 7011/768 |
| clear | 3904/53 | 2141/5 |
| aggregate | 2464/330 | - |
| edge_project | 980/123 | - |
| arrive | 494/28 | 2511/192 |
| edge_write | 411/45 | - |
| snapshot_pre | 367/14 | - |
| read_staged | 276/18 | - |
| edge_lookup | 193/41 | - |
| retention | 182/42 | - |
| intern | 129/26 | 155/6 |
| advance_tick | 104/14 | - |
| retraction_guard | - | 1128/3 |

Setup verbs, medians of 3 (excluded from tick-only):

| verb | ghcache sqlite us | wide_64 sqlite us |
| --- | --- | --- |
| ddl | 91362 | 48387 |
| boot | 6518 | 655 |
| unlabelled | 4172 | 5111 |

## Rust remainder per tick

Per-tick `wall us` is the median across 3 info runs. Per-tick `sqlite us` is from one trace run and covers the `execute` path only; the `execute_batch` path (clear, advance_tick, transaction begin/commit) is absent from this column and is folded into the aggregate clear/intern verbs above (~8% of ghcache tick SQLite, ~20% of wide_64).

ghcache:

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 6980 | 5408 | 1572 |
| 1 | 9350 | 7749 | 1601 |
| 2 | 8240 | 6647 | 1593 |
| 3 | 13100 | 10590 | 2510 |
| 4 | 10200 | 8406 | 1794 |
| 5 | 16200 | 12707 | 3493 |
| 6 | 7810 | 5212 | 2598 |
| 7 | 3450 | 2444 | 1006 |
| 8 | 2720 | 2040 | 680 |
| 9 | 2960 | 2009 | 951 |
| 10 | 2240 | 1313 | 927 |
| 11 | 8320 | 6232 | 2088 |
| 12 | 7200 | 5385 | 1815 |
| 13 | 1470 | 785 | 685 |

wide_64:

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 16600 | 12269 | 4331 |
| 1 | 8430 | 3551 | 4879 |
| 2 | 7690 | 3446 | 4244 |

## Run logs

Raw logs under `LAB-tick-verb-remeasure.runs/`:

| path | content |
| --- | --- |
| `ghcache_run{1,2,3}.err` | info run: seam tally, `DL_TRACE_SUMMARY`, per-tick wall |
| `ghcache_run{1,2,3}.out` | tick log |
| `wide64_run{1,2,3}.err` | info run: seam tally, `DL_TRACE_SUMMARY`, per-tick wall |
| `wide64_run{1,2,3}.out` | tick log |
| `ghcache_trace.err` | trace run: per-statement spans for per-tick sqlite |
| `wide64_trace.err` | trace run: per-statement spans for per-tick sqlite |
