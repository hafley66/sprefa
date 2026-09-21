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

## R5 the delta, and the answer

| cell | before range s | after range s | verdict |
|---|---|---|---|
| battery, rust engine | 61.53 to 66.19 | 119.18 to 121.21 | slower, 1.9x |
| battery, sqlite engine | 401.70 to 444.52 | 730.04 to 735.56 | slower, 1.7x |
| `_27_eval_sqlite`, rust | 12.83 to 13.04 | 28.57 to 28.61 | slower, 2.2x |
| `_27_eval_sqlite`, sqlite | 11.95 to 12.42 | 28.74 to 28.97 | slower, 2.4x |
| `_22_book`, rust | 12.74 to 16.75 | 10.98 to 11.95 | faster, 1.2x |
| `_22_book`, sqlite | 139.44 to 170.39 | 185.86 to 190.83 | slower, 1.3x |
| `_16_extract_tsi`, rust | 0.62 to 1.04 | 0.57 to 1.04 | in the noise |
| `_16_extract_tsi`, sqlite | 6.50 to 9.65 | 5.58 to 6.16 | faster, 1.3x |
| `dl8 comptime` biggest case, rust | 0.16 to 0.16 | 0.14 to 0.14 | faster, 1.1x |
| `dl8 comptime` biggest case, sqlite | 2.88 to 3.08 | 8.00 to 8.11 | slower, 2.7x |
| `dl8 comptime` prelude case, sqlite | 0.05 to 0.05 | 0.06 to 0.06 | slower, 1.2x |
| `dl8 compile fixtures/openapi/todo.dl7`, rust | 0.15 to 0.19 | 0.14 to 0.14 | faster, 1.1x |
| `dl8 compile fixtures/openapi/todo.dl7`, sqlite | 2.91 to 4.02 | 4.72 to 4.75 | slower, 1.4x |
| `dl8 compile fixtures/sqlite_emit/0_union_filter.dl7`, sqlite | 2.56 to 3.17 | 3.78 to 3.80 | slower, 1.3x |

**comptime got slower.** On the sqlite engine the whole battery is 1.7x slower
and the two comptime cells that matter are 1.4x and 2.7x slower. The default
rust engine is 1.9x slower on its battery because two targets inside it run the
sqlite engine. The rust engine itself, on the same programs, does not move or
moves 1.1x faster.

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