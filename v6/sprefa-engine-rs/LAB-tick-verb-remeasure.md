# LAB tick-verb-remeasure

Re-measure of the ghcache 14-tick fold and the wide_64 3-tick fold on current
main, per verb, medians of 3, splitting each microsecond of the fold wall into
SQLite time vs Rust time. No src/** change.

- [Method](#method)
- [Load context](#load-context)
- [ghcache, 14-tick fold](#ghcache-14-tick-fold)
- [wide_64, 3-tick fold](#wide_64-3-tick-fold)
- [Where the wall goes (SQLite vs Rust)](#where-the-wall-goes-sqlite-vs-rust)
- [Per-verb table](#per-verb-table)
- [Rust remainder per tick](#rust-remainder-per-tick)
- [Commands](#commands)
- [Raw run logs](#raw-run-logs)

## Method

All timing cells are `--release` medians of 3. Three sibling lanes were
building/testing on this machine during the first set (the loaded set); the
second set (the quiet set) ran only after `pgrep` reported no cargo/rustc and
the 1-minute load average held under 3.0 for three consecutive 30s polls. Both
sets are reported, labelled loaded and quiet. Load metrics (`uptime` load
average and `pgrep -c cargo`) were recorded before every run; see
[Load context](#load-context).

The fold is the `emit_rust_harness` one-shot (`--final`), the same fold line
`gate.sh` names, with `DL_TRACE_SUMMARY=1` so `sprefa_engine_rs::trace` prints
the per-verb table and the SQLite / wall totals at the end. `fold statements`
is the SEAM_TALLY per-tick sum from the `ordered_statement_count` test, which
equals seam-tally statements minus the ddl, boot, and unlabelled calls. It is a
count, not a time, so it is load-invariant: ghcache 6967, wide_64 2185 on both
sets.

The trace report's `TOTAL sqlite` and `TOTAL wall` include the one-time DDL and
boot. Each fold table therefore reports both the TOTAL and the tick-only slice,
the tick-only slice being TOTAL minus the ddl and boot rows. `tick-only SQLite
us` is the PROBE cell.

## Load context

`pgrep -c cargo` reads 0 on this macOS even while cargo and rustc are running (a
`pgrep -c` counting quirk), so it is recorded literally as instructed but is not
a usable discriminator. The reliable signal is the 1-minute load average plus
the exact cargo/rustc process counts, which the quiet-waiter gated on.

| set | run | time | load 1m / 5m / 15m | pgrep -c cargo | exact cargo / rustc |
| --- | --- | --- | --- | --- | --- |
| loaded | ghcache.1 | 13:47:19 | 4.64 / 6.47 / 6.03 | 0 | 1 / 3 |
| loaded | ghcache.2 | 13:47:39 | 5.16 / 6.18 / 5.95 | 0 | 1 / 3 |
| loaded | ghcache.3 | 13:47:39 | 5.16 / 6.18 / 5.95 | 0 | 1 / 3 |
| loaded | wide_64.1 | 13:47:39 | 5.16 / 6.18 / 5.95 | 0 | 1 / 3 |
| loaded | wide_64.2 | 13:47:40 | 5.16 / 6.18 / 5.95 | 0 | 1 / 3 |
| loaded | wide_64.3 | 13:47:40 | 5.16 / 6.18 / 5.95 | 0 | 1 / 3 |
| quiet | ghcache.1 | 14:12:15 | 2.72 / 4.53 / 5.41 | 0 | 0 / 0 |
| quiet | ghcache.2 | 14:12:23 | 2.82 / 4.52 / 5.40 | 0 | 0 / 0 |
| quiet | ghcache.3 | 14:12:23 | 2.82 / 4.52 / 5.40 | 0 | 0 / 0 |
| quiet | wide_64.1 | 14:12:15 | 2.72 / 4.53 / 5.41 | 0 | 0 / 0 |
| quiet | wide_64.2 | 14:12:23 | 2.82 / 4.52 / 5.40 | 0 | 0 / 0 |
| quiet | wide_64.3 | 14:12:23 | 2.82 / 4.52 / 5.40 | 0 | 0 / 0 |

The exact cargo/rustc counts are the `pgrep -x cargo` / `pgrep -x rustc` values
sampled in the same 30s window; the loaded set ran while a sibling lane
(`uniform-observability-knobs`) was mid `cargo test`.

## ghcache, 14-tick fold

Medians of 3, `--release`. PROBE 89e3074ee predates PRs #437-#441.

| cell | loaded | quiet | PROBE 89e3074ee |
| --- | --- | --- | --- |
| fold statements | 6967 | 6967 | 6738 |
| publish calls | 326 | 326 | 319 |
| publish us | 10947 | 10473 | 8714 |
| tick-only SQLite us | 92908 | 87642 | 77814 |
| tick-only wall us | 110088 | 106176 | - |
| TOTAL wall us | 212678 | 205058 | - |
| TOTAL wall ms | 212.7 | 205.1 | 91.3 |
| DDL SQLite us | 93487 | 91939 | - |
| boot SQLite us | 7046 | 6943 | - |

The PROBE reference predates PRs #437-#441. Even the quiet set on main is bigger
and slower than PROBE: statements 6967 vs 6738, tick SQLite 87642 vs 77814. The
loaded set adds roughly 5 ms of tick wall over quiet (110088 vs 106176). The
one-time DDL is the single largest wall cell on the run (91939-93487 us).

## wide_64, 3-tick fold

Medians of 3, `--release`.

| cell | loaded | quiet | PROBE 89e3074ee |
| --- | --- | --- | --- |
| fold statements | 2185 | 2185 | 2185 |
| publish calls | 384 | 384 | 384 |
| publish us | 6075 | 5827 | 5319 |
| tick-only SQLite us | 30358 | 29241 | 22875 |
| tick-only wall us | 35374 | 34051 | - |
| TOTAL wall us | 86656 | 84211 | - |
| TOTAL wall ms | 86.7 | 84.2 | 26.5 |
| DDL SQLite us | 50629 | 49730 | - |
| boot SQLite us | 653 | 616 | - |

wide_64 is a synthetic program untouched by #437-#441: statements and publish
calls are byte-identical to PROBE (2185, 384), yet tick-only SQLite grew 22875
to 29241 (quiet). That growth is engine-level, not a ghcache rel addition. The
DDL 49730-50629 us again dominates the run wall.

## Where the wall goes (SQLite vs Rust)

Fold-level split, medians of 3.

| set | corpus | TOTAL wall us | DDL us | boot us | tick wall us | tick SQLite us | tick Rust us |
| --- | --- | --- | --- | --- | --- | --- | --- |
| loaded | ghcache | 212678 | 93487 | 7046 | 110088 | 92908 | 17180 |
| quiet | ghcache | 205058 | 91939 | 6943 | 106176 | 87642 | 18534 |
| loaded | wide_64 | 86656 | 50629 | 653 | 35374 | 30358 | 5016 |
| quiet | wide_64 | 84211 | 49730 | 616 | 34051 | 29241 | 4810 |

Tick Rust remainder = tick wall minus tick SQLite: 15.6% of tick wall (ghcache
loaded), 17.5% (ghcache quiet), 14.2% (wide_64 loaded), 14.1% (wide_64 quiet).
The rest of the tick wall is SQLite.

## Per-verb table

Medians of 3, us/calls. `us` is the verb's wall (SQLite + Rust), `us_sql` is
the SQLite-only slice; `rust` is the difference.

### ghcache

| verb | loaded us/calls | loaded us_sql | quiet us/calls | quiet us_sql |
| --- | --- | --- | --- | --- |
| level_insert | 32219/1202 | 31190 | 31366/1202 | 30341 |
| recount | 20745/3358 | 18910 | 19969/3358 | 18134 |
| publish | 10947/326 | 9850 | 10473/326 | 9248 |
| probe | 10219/28 | 9661 | 9609/28 | 9131 |
| stage | 9445/558 | 7474 | 9078/558 | 7118 |
| clear | 4170/53 | 4156 | 4006/53 | 3995 |
| aggregate | 2676/330 | 2500 | 2575/330 | 2404 |
| edge_project | 1098/123 | 977 | 1015/123 | 916 |
| arrive | 683/28 | 585 | 619/28 | 523 |
| edge_write | 550/45 | 407 | 584/45 | 434 |
| snapshot_pre | 456/14 | 452 | 405/14 | 400 |
| read_staged | 308/18 | 296 | 272/18 | 261 |
| edge_lookup | 308/41 | 209 | 294/41 | 198 |
| intern | 281/26 | 147 | 245/26 | 134 |
| retention | 204/42 | 189 | 202/42 | 182 |
| advance_tick | 127/14 | 124 | 113/14 | 110 |

### wide_64

| verb | loaded us/calls | loaded us_sql | quiet us/calls | quiet us_sql |
| --- | --- | --- | --- | --- |
| stage | 8359/768 | 7036 | 8153/768 | 6819 |
| publish | 6075/384 | 5645 | 5827/384 | 5421 |
| level_insert | 3075/192 | 2985 | 3039/192 | 2940 |
| arrive | 2848/192 | 2585 | 2670/192 | 2424 |
| clear | 2266/5 | 2261 | 2256/5 | 2255 |
| probe | 2230/3 | 2123 | 2233/3 | 2145 |
| retraction_guard | 1174/3 | 1173 | 1122/3 | 1121 |
| intern | 263/6 | 157 | 251/6 | 147 |

`retention` is new since the PROBE table; it is the bounded-log prune verb added
by #437-#441. Per-verb SQLite dominates the Rust slice everywhere on ghcache
except `stage` (1971-2017 us Rust) and `recount` (1835 us Rust).

## Rust remainder per tick

Per tick: wall us is the clean (info-level) `tick` span median of 3; sqlite us
is the seam-span split from one trace-instrumented run, scaled so the column
sums to the tick-only SQLite median; remainder = wall minus sqlite. The per-tick
split is an estimate: a small negative remainder appears where the clean per-tick
wall and the scaled sqlite do not reconcile (trace-run wall differs from the
info-run wall per tick).

### ghcache, loaded

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 6580 | 5522 | 1058 |
| 1 | 9660 | 7805 | 1855 |
| 2 | 8030 | 6891 | 1139 |
| 3 | 14000 | 11566 | 2434 |
| 4 | 11100 | 11466 | -366 |
| 5 | 16800 | 14860 | 1940 |
| 6 | 8190 | 7574 | 616 |
| 7 | 4320 | 3462 | 858 |
| 8 | 3260 | 2523 | 737 |
| 9 | 3380 | 2797 | 583 |
| 10 | 2460 | 2122 | 338 |
| 11 | 9340 | 8388 | 952 |
| 12 | 8160 | 6689 | 1471 |
| 13 | 1510 | 1153 | 357 |
| TOTAL | 106790 | 92818 | 13972 |

### ghcache, quiet

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 6970 | 4928 | 2042 |
| 1 | 9850 | 7726 | 2124 |
| 2 | 7960 | 6540 | 1420 |
| 3 | 13100 | 12025 | 1075 |
| 4 | 10800 | 9072 | 1728 |
| 5 | 16400 | 14014 | 2386 |
| 6 | 7910 | 8085 | -175 |
| 7 | 3590 | 3363 | 227 |
| 8 | 2720 | 2266 | 454 |
| 9 | 3140 | 2469 | 671 |
| 10 | 2400 | 1870 | 530 |
| 11 | 8430 | 7298 | 1132 |
| 12 | 7780 | 6903 | 877 |
| 13 | 1530 | 1083 | 447 |
| TOTAL | 102580 | 87642 | 14938 |

### wide_64, loaded

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 16600 | 14879 | 1721 |
| 1 | 8940 | 7714 | 1226 |
| 2 | 8990 | 7765 | 1225 |
| TOTAL | 34530 | 30358 | 4172 |

### wide_64, quiet

| tick | wall us | sqlite us | remainder us |
| --- | --- | --- | --- |
| 0 | 16300 | 14547 | 1753 |
| 1 | 8090 | 7510 | 580 |
| 2 | 8340 | 7184 | 1156 |
| TOTAL | 32730 | 29241 | 3489 |

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

# fold, info level + trace summary (3x interleaved per set)
DL_ADAPTERS_DIR=v6/dl/ghcache RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness /tmp/ghcache.rs \
  v6/dl/ghcache/ghcache.schedule.json --final > out 2> err

RUST_LOG=sprefa_engine_rs=info DL_TRACE_SUMMARY=1 \
  timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness /tmp/wide_64.rs \
  v6/sprefa-engine-rs/tests/shared_frontier_wide/wide_64.schedule.json --final > out 2> err

# record load beside every run, before launching
uptime && pgrep -c cargo

# fold statements (ghcache): per-tick SEAM_TALLY sum
cargo test --release --test ordered_statement_count -- --nocapture

# per-tick split: trace-level seam spans + tick span closes
RUST_LOG=sprefa_engine_rs=trace timeout 60 v6/sprefa-engine-rs/target/release/emit_rust_harness \
  /tmp/ghcache.rs v6/dl/ghcache/ghcache.schedule.json --final 2> ghcache.trace.err
```

## Raw run logs

Under `LAB-tick-verb-remeasure.runs/`, two labeled sets:

| set | files | content |
| --- | --- | --- |
| `loaded/` | `ghcache.run{1,2,3}.{out,err,load}`, `wide_64.run{1,2,3}.{out,err,load}`, `ghcache.trace.err`, `wide_64.trace.err` | run during sibling build load (load 4.6-5.2, rustc active) |
| `quiet/` | `ghcache.run{1,2,3}.{out,err,load}`, `wide_64.run{1,2,3}.{out,err,load}`, `ghcache.trace.err`, `wide_64.trace.err` | run after cargo/rustc cleared and load held under 3.0 |

Each `.load` file carries the run's `uptime` and `pgrep -c cargo`. `.err`
carries the `== DL_TRACE_SUMMARY ==` table and the seam tally; `.out` carries
the tick log. Medians were computed off the six info-level runs per set and the
two trace-level runs per set.
