# sqlite_ivm refit

The dl8 engine loads sqlite_ivm as a dylib. This lane rebuilt that dylib from
`origin/main`, refit dl8 against it, and priced comptime on both dylibs.

## R1 the before dylib

| field | value |
|---|---|
| path dl8 resolves | `~/projects/sqlite_ivm/target/release/libsqlite_ivm.dylib` |
| bytes | 2839072 |
| mtime | 2026-09-20 02:07 |
| sha256 | `606e1a3e38cd1ac44c13ad33a20896de8b8d1bea95a542b7a140dd2a5eb205ab` |
| source | `2950cb6` on the `origin/main` line (PR #16, `fixpoint: emit stored representatives on retraction`) |
| built in | the sqlite_ivm worktree `.boop-worktrees/fix/fixpoint-emit`, clean at `5e431df` |
| gap to the new dylib | 94 commits, 18 merges, 15 pull-request merges |
| features cargo recorded | `["default", "extension"]` at that commit, where `default = []`. From `~/projects/sqlite_ivm/target/release/.fingerprint/sqlite-ivm-76e40aafba15c082/lib-sqlite_ivm.json` |
| SQLite linkage | `otool -L` reads `/usr/lib/libsqlite3.dylib (compatibility version 9.0.0, current version 358.0.0)` |
| sqlite3 symbols | one exported: `_sqlite3_extension_init`. Zero undefined. |

The artifact carries no commit. Its `.d` file, written with it at the same 02:07,
lists that worktree's `src/` paths, and its nine file names (`0_query.rs`,
`1_maintenance.rs`) are the layout before the engine split. That worktree's
manifest takes `hafley-observe` from the registry at 0.1.2 and has `default = []`.
`git diff 5e431df 2950cb6` is empty, so the source is the `origin/main` commit of
the same change.

Resolution order, first hit wins: `SQLITE_IVM_LIB`, then
`$CARGO_TARGET_DIR/release/libsqlite_ivm.dylib`, then
`sqlite_ivm/target/release/libsqlite_ivm.dylib` through the worktree symlink.

## R2 the new dylib

| field | value |
|---|---|
| source commit | `68a88321f4209c4523bcdc56ebb06ababcefebca` (sqlite_ivm `origin/main`) |
| source tree | `git archive origin/main`, byte for byte, no edit |
| bytes | 3112304 |
| sha256 | `f0b808ca0c5277b8b1776f06a9fe0059297bc7457f14d827e269d8e3c2e46a3e` |
| path | `$CARGO_TARGET_DIR/release/libsqlite_ivm.dylib` |
| build | `cargo build --release --offline --locked --no-default-features --features extension`, 36.8 s |
| features cargo recorded | `["extension"]`, from `$CARGO_TARGET_DIR/release/.fingerprint/sqlite-ivm-*/lib-sqlite_ivm.json` |
| SQLite linkage | `otool -L` reads `/usr/lib/libsqlite3.dylib (compatibility version 9.0.0, current version 358.0.0)` |
| sqlite3 symbols | one exported: `_sqlite3_extension_init`. Zero undefined. |
| loaded at run time | `lsof` on a live test child shows the 3112304 byte file |

`origin/main` makes `bundled` a default feature and its own
`scripts/10_package.sh` builds extensions with `--no-default-features`. The
manifest reaches two `hafley-rs` crates by a path four levels up, so the build
ran from a scratch copy whose four levels up is the lane's `hafley-rs` symlink.

## R3 the battery

`cargo test --no-fail-fast` at the root, logging off, one run at a time, three
runs each cell. The before phase carries one extra run: the first one built the
`sprefa-extract` binary `_16_extract_tsi` needs (83.1 s inside `_16`), so it
reads 151.82 s and is left out of the triple.

| engine | phase | run 1 s | run 2 s | run 3 s | peak RSS MB |
|---|---|---|---|---|---|
| rust | before | 66.19 | 65.51 | 61.53 | 1094 1434 1159 |
| rust | after | 121.21 | 119.18 | 119.93 | 1582 1558 1585 |
| sqlite | before | 421.68 | 444.52 | 401.70 | 1309 1245 1299 |
| sqlite | after | 733.12 | 730.04 | 735.56 | 1726 1588 1477 |

Every after triple falls outside its before spread. The rust row moves on the
new library because two targets inside it run the sqlite engine.

| target | engine | before s | after s | delta |
|---|---|---|---|---|
| `(lib)` unit tests | rust | 22.20 | 58.24 | +36.0 |
| `_27_eval_sqlite` | rust | 11.90 | 30.04 | +18.1 |
| all other rust targets | rust | 32.0 | 31.7 | flat, each inside 1 s |
| `_8_compile_oracle` | sqlite | 74.73 | 147.95 | +73.2 |
| `_22_book` | sqlite | 127.80 | 187.60 | +59.8 |
| `_18_sqlite_emit` | sqlite | 26.61 | 50.01 | +23.4 |
| `_27_eval_sqlite` | sqlite | 11.42 | 29.60 | +18.2 |
| `_9_tracing` | sqlite | 11.17 | 23.53 | +12.4 |
| `_17_reconcile` | sqlite | 9.64 | 20.23 | +10.6 |
| `_20_hosts` | sqlite | 18.42 | 28.78 | +10.4 |
| `_6_comptime_oracle` | sqlite | 2.95 | 8.72 | +5.8 |
| `_2_macrotime_oracle` | sqlite | 5.99 | 10.83 | +4.8 |
| `_23_comptime_effect` | sqlite | 3.07 | 6.40 | +3.3 |
| `_21_openapi` | sqlite | 3.13 | 5.69 | +2.6 |
| `_16_extract_tsi` | sqlite | 7.86 | 6.52 | -1.3 |

Every sqlite target that compiles a fixture roughly doubles. The per-target
before and after rows are run 3 of each phase.

### failures, triaged

No leg failed only after the swap. No bucket 2 and no bucket 3 finding.

| leg | engine | old dylib | new dylib | bucket |
|---|---|---|---|---|
| `_16_extract_tsi` | rust | red 3/3 | red 3/3 | already red, `CI-KNOWN-RED.md` dl8 battery row |
| `_22_book::probes_compile_as_their_page_says` | rust | red 3/3 | red 3/3 | already red, same file |
| `_15_fold`, both tests | sqlite | red 3/3 | red 3/3 | already red with the old dylib |
| `_16_extract_tsi` | sqlite | red 3/3 | red 3/3 | already red with the old dylib |
| `_22_book` chapters | sqlite | red 3/3 | red 3/3 | already red with the old dylib |
| `_20_hosts::extract_answers_rows_for_each_family_over_the_corpus` | rust | green | green | the `CI-KNOWN-RED` row does not reproduce here |

`_15_fold` fails on the sqlite engine and passes on the rust engine with both
dylibs: `1_program_step: rows differ`, `got: []`. That is a dl8 gap in the
sqlite eval path, not a library change, and it predates this lane.

Oracle parity through the real binary, `_27_eval_sqlite -- --nocapture`:

| dylib | result |
|---|---|
| old | `sqlite oracles 32/32` |
| new | `sqlite oracles 32/32` |

## R4 the after numbers, same cells as R1

One process per run, three runs each, wall seconds.

| cell | engine | before | after |
|---|---|---|---|
| `dl8 comptime` biggest case, 1.8 MB | rust | 0.16 0.16 0.16 | 0.14 0.14 0.14 |
| `dl8 comptime` biggest case, 1.8 MB | sqlite | 3.08 2.88 2.89 | 8.02 8.00 8.11 |
| `dl8 comptime` prelude case | rust | 0.01 0.01 0.01 | 0.01 0.01 0.01 |
| `dl8 comptime` prelude case | sqlite | 0.05 0.05 0.05 | 0.06 0.06 0.06 |
| `dl8 compile fixtures/openapi/todo.dl7` | rust | 0.19 0.15 0.15 | 0.14 0.14 0.14 |
| `dl8 compile fixtures/openapi/todo.dl7` | sqlite | 2.91 3.79 4.02 | 4.72 4.75 4.72 |
| `dl8 compile fixtures/sqlite_emit/0_union_filter.dl7` | rust | 0.12 0.12 0.12 | 0.12 0.11 0.11 |
| `dl8 compile fixtures/sqlite_emit/0_union_filter.dl7` | sqlite | 2.56 2.60 3.17 | 3.78 3.80 3.79 |

Peak RSS of one process, MB.

| cell | engine | before | after |
|---|---|---|---|
| `dl8 comptime` biggest case, 1.8 MB | rust | 112 121 116 | 122 113 117 |
| `dl8 comptime` biggest case, 1.8 MB | sqlite | 792 939 771 | 1440 1438 1540 |
| `dl8 compile fixtures/openapi/todo.dl7` | rust | 83 80 80 | 83 81 83 |
| `dl8 compile fixtures/openapi/todo.dl7` | sqlite | 755 536 355 | 1256 1257 1229 |

Disk: no harness reports bytes. One `dl8 eval --db` file per engine per phase
reads 786432 bytes in all four cases. That file is dl8's own row store; the
sqlite engine's database is `:memory:`, so it writes nothing of its own.

## R4b the commands and their recorded stdout

The four runners live under `$CARGO_TARGET_DIR/refit` and are outside git, so
they are pasted whole here with the method. `<engine>` unset means `DL8_ENGINE`
is unset in the child environment.

`run-battery.sh <label> <engine> <n>`

```bash
ROOT=/Users/chrishafley/projects/sprefa/.boop-worktrees/feature/sqlite-ivm-refit
OUT="${CARGO_TARGET_DIR}/refit"
log="$OUT/battery-$label-$engine-$n.log"
timef="$OUT/battery-$label-$engine-$n.time"
cd "$ROOT" || exit 9
if [ "$engine" = sqlite ]; then
  env -u RUST_LOG -u HAFLEY_LOG -u DL8_EVAL_CHECK -u SQLITE_IVM_LIB \
    DL8_ENGINE=sqlite \
    /usr/bin/time -l -o "$timef" cargo test --no-fail-fast >"$log" 2>&1
else
  env -u RUST_LOG -u HAFLEY_LOG -u DL8_EVAL_CHECK -u SQLITE_IVM_LIB -u DL8_ENGINE \
    /usr/bin/time -l -o "$timef" cargo test --no-fail-fast >"$log" 2>&1
fi
```

`run-leg.sh <label> <engine> <n> <test file>` is the same with
`cargo test --test "$leg"`. `run-comptime.sh <label>` runs, for each engine and
each of three runs:

```bash
env -u RUST_LOG -u HAFLEY_LOG -u DL8_EVAL_CHECK -u SQLITE_IVM_LIB DL8_ENGINE=sqlite \
  /usr/bin/time -l -o "$timef" "$BIN" comptime "$cell" >"$log" 2>&1
```

with `$BIN` = `$CARGO_TARGET_DIR/debug/dl8` and `$cell` one of
`oracle/comptime/2_partial_evaluate_checked_21.json`,
`oracle/comptime/prelude_evaluate_checked.json`. `run-programs.sh <label>` runs
the same shape with `compile fixtures/openapi/todo.dl7` and
`compile fixtures/sqlite_emit/0_union_filter.dl7`. The oracle-count run is
`cargo test --test _27_eval_sqlite -- --nocapture`; the differ run is
`DL8_ENGINE=sqlite DL8_EVAL_CHECK=1 "$BIN" <cell>`.

Recorded stdout, verbatim. `/usr/bin/time -l` writes `real/user/sys` and `maximum
resident set size`; the run log carries the rest.

whole battery, before, old dylib

```
DL8_ENGINE unset, run 1:  151.82 real       259.91 user        37.51 sys   (cold: builds sprefa-extract)
                            1171079168  maximum resident set size
DL8_ENGINE unset, run 2:   66.19 real        78.00 user        16.50 sys
                            1139507200  maximum resident set size
DL8_ENGINE unset, run 3:   65.51 real        77.42 user        16.69 sys
                            1159462912  maximum resident set size
DL8_ENGINE unset, run 4:   61.53 real        75.40 user        14.44 sys
                            1433649152  maximum resident set size
error: test failed, to rerun pass `--test _16_extract_tsi`
error: test failed, to rerun pass `--test _22_book`

DL8_ENGINE=sqlite, run 1:  421.68 real       705.55 user        79.80 sys
                            1308803072  maximum resident set size
DL8_ENGINE=sqlite, run 2:  444.52 real       716.68 user        85.35 sys
                            1244889088  maximum resident set size
DL8_ENGINE=sqlite, run 3:  401.70 real       674.90 user        64.55 sys
                            1298530304  maximum resident set size
error: test failed, to rerun pass `--test _15_fold`
error: test failed, to rerun pass `--test _16_extract_tsi`
error: test failed, to rerun pass `--test _22_book`
```

whole battery, after, new dylib

```
DL8_ENGINE unset, run 1:  121.21 real       126.74 user        21.17 sys
                            1658208256  maximum resident set size
DL8_ENGINE unset, run 2:  119.18 real       125.68 user        21.49 sys
                            1633337344  maximum resident set size
DL8_ENGINE unset, run 3:  119.93 real       126.00 user        21.52 sys
                            1662402560  maximum resident set size
error: test failed, to rerun pass `--test _16_extract_tsi`
error: test failed, to rerun pass `--test _22_book`

DL8_ENGINE=sqlite, run 1:  733.12 real      1179.43 user       115.24 sys
                            1809399808  maximum resident set size
DL8_ENGINE=sqlite, run 2:  730.04 real      1174.01 user       112.82 sys
                            1665744896  maximum resident set size
DL8_ENGINE=sqlite, run 3:  735.56 real      1180.90 user       117.22 sys
                            1549336576  maximum resident set size
error: test failed, to rerun pass `--test _15_fold`
error: test failed, to rerun pass `--test _16_extract_tsi`
error: test failed, to rerun pass `--test _22_book`
```

`_22_book`, alone, new and old

```
old, DL8_ENGINE unset, run 1:  16.75 real    22.86 user     6.47 sys   224411648 rss
old, DL8_ENGINE unset, run 2:  15.26 real    22.96 user     6.38 sys   222298112 rss
old, DL8_ENGINE unset, run 3:  12.74 real    22.93 user     5.88 sys   238518272 rss
  test result: FAILED. 22 passed; 1 failed
new, DL8_ENGINE unset, run 1:  11.17 real    20.37 user     4.09 sys   238436352 rss
new, DL8_ENGINE unset, run 2:  10.98 real    20.57 user     4.43 sys   234389504 rss
new, DL8_ENGINE unset, run 3:  11.95 real    21.09 user     5.21 sys   232308736 rss
  test result: FAILED. 22 passed; 1 failed
old, DL8_ENGINE=sqlite, run 1: 170.39 real   232.15 user    28.82 sys   774701056 rss
old, DL8_ENGINE=sqlite, run 2: 149.13 real   299.35 user    32.01 sys   790855680 rss
old, DL8_ENGINE=sqlite, run 3: 139.44 real   284.42 user    29.39 sys   976961536 rss
  test result: FAILED. 8/12/11 passed of 23
new, DL8_ENGINE=sqlite, run 1: 190.83 real   395.75 user    40.27 sys  1346387968 rss
new, DL8_ENGINE=sqlite, run 2: 185.90 real   391.41 user    34.39 sys  1351892992 rss
new, DL8_ENGINE=sqlite, run 3: 185.86 real   391.88 user    34.84 sys  1357660160 rss
  test result: FAILED. 8/9/9 passed of 23
```

`dl8 comptime` on the 1.8 MB case,
`oracle/comptime/2_partial_evaluate_checked_21.json`

```
old, DL8_ENGINE unset, run 1:   0.16 real     0.13 user     0.02 sys   119832576 rss
old, DL8_ENGINE unset, run 2:   0.16 real     0.13 user     0.02 sys   117407744 rss
old, DL8_ENGINE unset, run 3:   0.16 real     0.13 user     0.02 sys   126976000 rss
new, DL8_ENGINE unset, run 1:   0.14 real     0.12 user     0.01 sys   127795200 rss
new, DL8_ENGINE unset, run 2:   0.14 real     0.12 user     0.01 sys   118833152 rss
new, DL8_ENGINE unset, run 3:   0.14 real     0.12 user     0.01 sys   122191872 rss
old, DL8_ENGINE=sqlite, run 1:  3.08 real     2.71 user     0.30 sys   830799872 rss
old, DL8_ENGINE=sqlite, run 2:  2.88 real     2.62 user     0.23 sys   984481792 rss
old, DL8_ENGINE=sqlite, run 3:  2.89 real     2.63 user     0.24 sys   809140224 rss
new, DL8_ENGINE=sqlite, run 1:  8.02 real     7.32 user     0.68 sys  1510031360 rss
new, DL8_ENGINE=sqlite, run 2:  8.00 real     7.25 user     0.73 sys  1507459072 rss
new, DL8_ENGINE=sqlite, run 3:  8.11 real     7.34 user     0.75 sys  1614659584 rss
```

`dl8 compile fixtures/openapi/todo.dl7`

```
old, DL8_ENGINE unset, run 1:   0.19 real     0.15 user     0.01 sys    86982656 rss
old, DL8_ENGINE unset, run 2:   0.15 real     0.13 user     0.01 sys    84361216 rss
old, DL8_ENGINE unset, run 3:   0.15 real     0.13 user     0.01 sys    83574784 rss
new, DL8_ENGINE unset, run 1:   0.14 real     0.13 user     0.01 sys    86736896 rss
new, DL8_ENGINE unset, run 2:   0.14 real     0.13 user     0.01 sys    85000192 rss
new, DL8_ENGINE unset, run 3:   0.14 real     0.13 user     0.01 sys    87359488 rss
old, DL8_ENGINE=sqlite, run 1:  2.91 real     2.68 user     0.20 sys   792264704 rss
old, DL8_ENGINE=sqlite, run 2:  3.79 real     2.97 user     0.27 sys   561938432 rss
old, DL8_ENGINE=sqlite, run 3:  4.02 real     3.04 user     0.31 sys   372817920 rss
new, DL8_ENGINE=sqlite, run 1:  4.72 real     4.39 user     0.32 sys  1317388288 rss
new, DL8_ENGINE=sqlite, run 2:  4.75 real     4.38 user     0.35 sys  1317584896 rss
new, DL8_ENGINE=sqlite, run 3:  4.72 real     4.38 user     0.32 sys  1288454144 rss
```

## R5 the delta, and the answer

Floor and denominator, stated once: the denominator is the before spread (max
minus min) of the same cell. A delta lands as a change only if all three after
runs fall outside the before range and the median-to-median delta clears 1.0 s on
a one-process cell or 10 s on a battery. Anything under those reads no effect at
this scale, numbers shown.

| cell | before median | after median | delta | before spread | verdict |
|---|---|---|---|---|---|
| battery, rust engine | 65.51 | 119.93 | +54.42 | 4.66 | slower, 1.83x |
| battery, sqlite engine | 421.68 | 733.12 | +311.44 | 42.82 | slower, 1.74x |
| `_27_eval_sqlite`, rust | 12.91 | 28.60 | +15.69 | 0.21 | slower, 2.21x |
| `_27_eval_sqlite`, sqlite | 12.04 | 28.75 | +16.71 | 0.47 | slower, 2.39x |
| `_22_book`, rust | 15.26 | 11.17 | -4.09 | 4.01 | faster 1.37x on the medians; the before triple falls 16.75 15.26 12.74, so the gap is not separable from warm-up |
| `_22_book`, sqlite | 149.13 | 185.90 | +36.77 | 30.95 | slower, 1.25x |
| `_16_extract_tsi`, rust | 0.64 | 0.59 | -0.05 | 0.42 | no effect at this scale |
| `_16_extract_tsi`, sqlite | 8.12 | 5.67 | -2.45 | 3.15 | no effect at this scale: the delta is inside the before spread |
| `dl8 comptime` biggest case, rust | 0.16 | 0.14 | -0.02 | 0.00 | no effect at this scale |
| `dl8 comptime` biggest case, sqlite | 2.89 | 8.02 | +5.13 | 0.20 | slower, 2.78x |
| `dl8 comptime` prelude case, sqlite | 0.05 | 0.06 | +0.01 | 0.00 | no effect at this scale |
| `dl8 compile fixtures/openapi/todo.dl7`, rust | 0.15 | 0.14 | -0.01 | 0.04 | no effect at this scale |
| `dl8 compile fixtures/openapi/todo.dl7`, sqlite | 3.79 | 4.72 | +0.93 | 1.11 | no effect at this scale: 0.93 s is under the 1.0 s floor, though the ranges are disjoint |
| `dl8 compile fixtures/sqlite_emit/0_union_filter.dl7`, sqlite | 2.60 | 3.79 | +1.19 | 0.61 | slower, 1.46x |

**comptime got slower.** On the sqlite engine the whole battery is 1.74x slower
and the one comptime cell that clears the floor is 2.78x slower. The default rust
engine is 1.83x slower on its battery because two targets inside it run the
sqlite engine. The rust engine by itself, on the same programs, does not move.

### is this like for like

Yes. Asked and answered from three directions, because a bundled-SQLite dylib
against a host-SQLite dylib would make the whole table a measurement of a build
flag.

| receipt | old dylib | new dylib |
|---|---|---|
| features cargo recorded in the fingerprint of the build that wrote the artifact | `["default","extension"]`, where that commit's `default = []` | `["extension"]` |
| `otool -L` | `/usr/lib/libsqlite3.dylib (9.0.0, 358.0.0)` | `/usr/lib/libsqlite3.dylib (9.0.0, 358.0.0)` |
| exported `sqlite3` symbols | `_sqlite3_extension_init` only | `_sqlite3_extension_init` only |
| undefined `sqlite3` symbols | 0 | 0 |

Neither artifact carries a copy of SQLite: a bundled build exports the SQLite API
and drops the `libsqlite3.dylib` load command. The old manifest enables `default`,
which is `[]`, plus `extension`; it has no `bundled` feature to enable, and
`rusqlite` 0.40.2's own defaults are `["cache","ffi-sqlite-wasm-rs"]`, so nothing
could pull the bundled library in. `origin/main` added `default = ["bundled"]`,
which is why the extension recipe now passes `--no-default-features`: it is what
keeps the new build equal to the old one on this axis, not what makes it differ.

### the cell that moved

The cell is **declare**. dl8 declares one fresh view per program and again per
comptime round, and the new engine charges more for that. Same cell, same
statements, one `dl8 compile fixtures/openapi/todo.dl7`, millisecond sums from
the `dl8::sql` spans, old dylib against new:

| statement | old ms | new ms |
|---|---|---|
| `declare_view`, the `CREATE VIRTUAL TABLE` | 1427.9 | 3168.7 |
| `flush_term_args` | 600.0 | 642.2 |
| `flush_terms` | 267.3 | 277.5 |
| `nonlinear_round` | 215.8 | 235.6 |
| `insert_seeds`, the first seed insert | 19.3 | 178.5 |
| `declare_tables` | 12.4 | 12.6 |
| total | 2549.8 | 4519.5 |

The statement count is identical, 1275 `dl8::sql` spans with the same histogram
of names, so the new engine does the same number of things more slowly from
dl8's side: 2.2x on the declaration and 9x on the first seed insert. The engine
work in the gap is maintenance work: no mutation rebuilds the view, set-at-a-time
drains, a prepared statement program per view, delete-and-rederive for recursive
strata. dl8's comptime never mutates a declared view; it throws the view away
and declares a new one. So the engine work paid off where dl8 does not reach.

## R6 `DL8_EVAL_CHECK=1`

Same result with both dylibs. Three cells through the real binary, the sqlite
engine, the differ on:

| cell | `fresh-only row` | `cached-only row` |
|---|---|---|
| `dl8 comptime` prelude case | 0 | 0 |
| `dl8 comptime` biggest case | 0 | 0 |
| `dl8 compile fixtures/openapi/todo.dl7` | 0 | 0 |

The built-in rail `_6_eval::sqlite_eval::incremental::a_seed_delta_reads_like_a_fresh_view`,
which compares a staged insert against a fresh view over all 32 oracle programs,
passes with both dylibs. So the sqlite path agrees with a fresh view on the
insert path and on every cell a CLI verb reaches. The delete path, which
`_7_sqlite_eval.rs` names as the disagreeing one, is only reachable through the
dev server's file reload; no CLI verb here reaches it. That rail does get slower
with the new dylib: the unit target it lives in goes 22.20 s to 58.24 s.

## R7 clippy

`cargo clippy --all-targets -- -D warnings` exits 101 at `origin/main` and at
this lane's tree. This lane changes no `src/` file, so the diff adds no finding.

| file | findings | owned |
|---|---|---|
| `src/_5_reify/_7_sqlite.rs` | 6 | yes |
| `src/_6_eval/_7_sqlite_eval.rs` | 5 | yes |
| `src/_6_eval/_8_functions.rs` | 1 | no |
| `src/_3_check/_5_kernel.rs` | 1 | no |

`git diff --stat origin/main -- src` is empty. The eleven findings inside the two
owned files are pre-existing at `e0eaec6` and are untouched: the refit needed no
source change.

## R8 the diff

```
justfile                      1 file changed, 2 insertions(+), 1 deletion(-)
plans/v8/2026-09-21-sqlite-ivm-refit.md                        new
plans/v8/2026-09-21-sqlite-ivm-refit.visual.human.unga.md      new
```

Nothing under `sqlite_ivm/`. No `Cargo.toml`, no `Cargo.lock`.

The justfile edit is the recipe correction: `origin/main` makes `bundled` a
default feature, so the extension build needs `--no-default-features`, which the
`ivm-ext` recipe now passes.

## Notes for the next lane

Two measurement hazards, both hit here:

- `tests/_22_book.rs` kills only the `bash -c` it spawns, so the `dl8 run
  --serve git.refs` grandchild survives every run of that file, blocked in
  `soopy::RepositoryWatcher::recv_timeout`. Several accumulated here before
  cleanup. They cost no CPU, but they hold watch handles.
- The first battery run after a fresh `sprefa-extract` build carries about 83 s
  of that build inside `_16_extract_tsi`. Discard it, as R3 does.

One unrelated change to watch: a cargo run in this worktree re-resolved
`Cargo.lock` once, dropping the source suffix from the local `hafley` registry
entry and moving a Windows-only `windows-sys` leaf. `git status` shows it as a
modification; this lane restored the file and never staged it. No crate was
recompiled: every test binary under `target/debug/deps` still carries the 10:29
build time, and no timed run printed `Compiling`. The statement-level table in R5
was measured with both dylibs selected by `SQLITE_IVM_LIB` against the one
binary, which isolates the library from that change.