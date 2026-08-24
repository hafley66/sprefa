# LAB tick-verb-remeasure

Re-measure of the ghcache 14-tick fold and the wide_64 3-tick fold on current
main, per verb, medians of 3, splitting each microsecond of the fold wall into
SQLite time vs Rust time. No src/** change.

- [Method](#method)
- [ghcache, 14-tick fold](#ghcache-14-tick-fold)
- [wide_64, 3-tick fold](#wide_64-3-tick-fold)
- [Where the wall goes (SQLite vs Rust)](#where-the-wall-goes-sqlite-vs-rust)
- [Per-verb table](#per-verb-table)
- [Rust remainder per tick](#rust-remainder-per-tick)
- [Commands](#commands)
- [Raw run logs](#raw-run-logs)

## Method

All timing cells are `--release` medians of 3, runs interleaved
(ghcache, wide_64, ghcache, wide_64, ghcache, wide_64). The fold is the
`emit_rust_harness` one-shot (`--final`), the same fold line `gate.sh` names,
with `DL_TRACE_SUMMARY=1` so `sprefa_engine_rs::trace` prints the per-verb
table and the SQLite / wall totals at the end. `fold statements` is the
SEAM_TALLY per-tick sum from the `ordered_statement_count` test, which equals
seam-tally statements minus the ddl, boot, and unlabelled calls.

The trace report's `TOTAL sqlite` and `TOTAL wall` include the one-time DDL and
boot (which current main instruments and which dominate the run). Each fold
table therefore reports both the TOTAL and the tick-only slice, the tick-only
slice being TOTAL minus the ddl and boot rows. `tick-only SQLite us` is the
PROBE cell; it is the tick portion of the fold.

## ghcache, 14-tick fold

Medians of 3, `--release`.

| cell | main (this branch) | PROBE 89e3074ee |
| --- | --- | --- |
| fold statements | 6967 | 6738 |
| publish calls | 326 | 319 |
| publish us | 10772 | 8714 |
| tick-only SQLite us | 93954 | 77814 |
| tick-only wall us | 113068 | - |
| TOTAL wall us | 217936 | - |
| TOTAL wall ms | 217.9 | 91.3 |
| DDL SQLite us | 97979 | - |
| boot SQLite us | 6772 | - |

The PROBE reference predates PRs #437-#441. The fold on main today is bigger
and slower: statements 6967 vs 6738, tick SQLite 93954 vs 77814. The one-time
DDL (schema for the added rels and the retention triggers) is the single
largest wall cell on the run: 97979 us of the 217936 us total.

## wide_64, 3-tick fold

Medians of 3, `--release`.

| cell | main (this branch) | PROBE 89e3074ee |
| --- | --- | --- |
| fold statements | 2185 | 2185 |
| publish calls | 384 | 384 |
| publish us | 6290 | 5319 |
| tick-only SQLite us | 33421 | 22875 |
| tick-only wall us | 40580 | - |
| TOTAL wall us | 97013 | - |
| TOTAL wall ms | 97.0 | 26.5 |
| DDL SQLite us | 55154 | - |
| boot SQLite us | 677 | - |

wide_64 is a synthetic program untouched by #437-#441: statements and publish
calls are byte-identical to PROBE (2185, 384), yet tick-only SQLite grew 22875
to 33421. That growth is engine-level, not a ghcache rel addition. The DDL
55154 us again dominates the run wall.

## Where the wall goes (SQLite vs Rust)

Fold-level split, medians of 3.

| corpus | TOTAL wall us | DDL us | boot us | tick wall us | tick SQLite us | tick Rust us |
| --- | --- | --- | --- | --- | --- | --- |
| ghcache | 217936 | 97979 | 6772 | 113068 | 93954 | 19114 |
| wide_64 | 97013 | 55154 | 677 | 40580 | 33421 | 7159 |

Tick Rust remainder = tick wall minus tick SQLite: 16.9% of tick wall on
ghcache, 17.6% on wide_64. The rest of the tick wall is SQLite.

## Per-verb table

Medians of 3, us/calls. `us` is the verb's wall (SQLite + Rust), `us_sql` is
the SQLite-only slice; `rust` is the difference.

| verb | ghcache us/calls | ghcache us_sql | wide_64 us/calls | wide_64 us_sql |
| --- | --- | --- | --- | --- |
| level_insert | 32593/1202 | 31629 | 3153/192 | 3056 |
| recount | 20773/3358 | 18909 | - | - |
| publish | 10772/326 | 9673 | 6290/384 | 5840 |
| probe | 10127/28 | 9536 | 2344/3 | 2216 |
| stage | 9282/558 | 7265 | 8763/768 | 7433 |
| clear | 4238/53 | 4223 | 2513/5 | 2512 |
| aggregate | 2700/330 | 2524 | - | - |
| edge_project | 1104/123 | 997 | - | - |
| arrive | 670/28 | 574 | 2970/192 | 2686 |
| edge_write | 568/45 | 425 | - | - |
| snapshot_pre | 454/14 | 448 | - | - |
| edge_lookup | 318/41 | 218 | - | - |
| read_staged | 299/18 | 286 | - | - |
| intern | 275/26 | 156 | 311/6 | 163 |
| retention | 215/42 | 196 | - | - |
| advance_tick | 136/14 | 133 | - | - |
| retraction_guard | - | - | 1258/3 | 1256 |

`retention` is new since the PROBE table; it is the bounded-log prune verb
added by #437-#441. Per-verb SQLite dominates the Rust slice everywhere on
ghcache except `stage` (2017 us Rust of 9282) and `recount` (1864 us Rust of
20773).

## Rust remainder per tick

Per tick: wall us is the clean (info-level) `tick` span median of 3; sqlite us
is the seam-span split from one trace-instrumented run, scaled so the column
sums to the tick-only SQLite median; remainder = wall minus sqlite.

ghcache, 14 ticks:

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 7360 | 5590 | 1770 |
| 1 | 9540 | 7901 | 1639 |
| 2 | 8140 | 6976 | 1164 |
| 3 | 13800 | 11708 | 2092 |
| 4 | 11900 | 11606 | 294 |
| 5 | 16800 | 15043 | 1757 |
| 6 | 8360 | 7668 | 692 |
| 7 | 4310 | 3505 | 805 |
| 8 | 2920 | 2554 | 366 |
| 9 | 3520 | 2831 | 689 |
| 10 | 2430 | 2149 | 281 |
| 11 | 9430 | 8491 | 939 |
| 12 | 8240 | 6771 | 1469 |
| 13 | 1770 | 1167 | 603 |
| TOTAL | 109811 | 93954 | 15857 |

wide_64, 3 ticks:

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 17800 | 16380 | 1420 |
| 1 | 9330 | 8492 | 838 |
| 2 | 9590 | 8548 | 1042 |
| TOTAL | 36720 | 33420 | 3300 |

The busy ticks (ghcache 3, 4, 5) carry the bulk of the sqlite time; the Rust
remainder is roughly flat and small per tick.

## Commands

```bash
# build once, release
cd v6/sprefa-engine-rs && cargo build --release --bin emit_rust_harness

# compile the two programs (prolog emitter)
swipl --stack_limit=12G -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile_dl6('v6/dl/ghcache/ghcache.dl6','/tmp/ghcache.rs',[emitter(emit_rust:emit_program)])" -g halt
swipl --stack_limit=12G -q -l v6/prolog/compile.pl -l v6/prolog/emit_rust.pl \
  -g "compile_dl6('v6/sprefa-engine-rs/tests/shared_frontier_wide/wide_64.dl6','/tmp/wide_64.rs',[emitter(emit_rust:emit_program)])" -g halt

# fold, info level + trace summary (interleaved 3x each)
DL_ADAPTERS_DIR=v6/dl/ghcache RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness /tmp/ghcache.rs \
  v6/dl/ghcache/ghcache.schedule.json --final > out 2> err

RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness /tmp/wide_64.rs \
  v6/sprefa-engine-rs/tests/shared_frontier_wide/wide_64.schedule.json --final > out 2> err

# fold statements (ghcache): per-tick SEAM_TALLY sum
cargo test --release --test ordered_statement_count -- --nocapture

# per-tick split: trace-level seam spans + tick span closes
RUST_LOG=sprefa_engine_rs=trace timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness \
  /tmp/ghcache.rs v6/dl/ghcache/ghcache.schedule.json --final 2> ghcache.trace.err
```

## Raw run logs

Under `LAB-tick-verb-remeasure.runs/`:

| file | content |
| --- | --- |
| `ghcache.run{1,2,3}.out` / `.err` | ghcache fold, info + trace summary, x3 |
| `wide_64.run{1,2,3}.out` / `.err` | wide_64 fold, info + trace summary, x3 |
| `ghcache.trace.err` | ghcache fold at trace level (per-tick seam spans) |
| `wide_64.trace.err` | wide_64 fold at trace level (per-tick seam spans) |

`.err` carries the `== DL_TRACE_SUMMARY ==` table and the seam tally; `.out`
carries the tick log. Medians were computed off these six info-level runs and
the two trace-level runs.
