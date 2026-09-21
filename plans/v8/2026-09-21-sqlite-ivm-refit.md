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

Evidence: `target/release/libsqlite_ivm.d`, written with the artifact at the same
02:07, lists that worktree's `src/` paths. Its nine file names (with `0_query.rs`
and `1_maintenance.rs`) are the pre-split layout. Its manifest still takes
`hafley-observe` from the registry at 0.1.2 and has `default = []`, so the old
build needed no feature flags. `git diff 5e431df 2950cb6` is empty.

Resolution order dl8 uses, first hit wins: `SQLITE_IVM_LIB`, then
`$CARGO_TARGET_DIR/release/libsqlite_ivm.dylib`, then
`sqlite_ivm/target/release/libsqlite_ivm.dylib` through the worktree symlink.

## R2 the new dylib

| field | value |
|---|---|
| source commit | `68a88321f4209c4523bcdc56ebb06ababcefebca` (`sqlite_ivm` `origin/main`) |
| source tree | `git archive origin/main`, no edit to it |
| bytes | 3112304 |
| sha256 | `f0b808ca0c5277b8b1776f06a9fe0059297bc7457f14d827e269d8e3c2e46a3e` |
| path | `$CARGO_TARGET_DIR/release/libsqlite_ivm.dylib` |
| build | `cargo build --release --offline --locked --no-default-features --features extension`, 36.8 s |
| build sha verified at run time | `lsof` on live test children shows the 3112304 byte file |

`origin/main` makes `bundled` a default feature and its own scripts build the
extension with `--no-default-features`, so the extension build needs that flag.
The manifest reaches its two `hafley-rs` crates by a relative path four levels
up; the build ran from a scratch copy whose four levels up is the lane root
`hafley-rs` symlink. No repo file was edited for the build.

## R3 the battery, three runs each engine each phase

`cargo test --no-fail-fast` at the root, logging off, one run at a time.
Before phase carries one extra cold-cache run (the `_16_extract_tsi` extract
build, 83.1 s, runs once per machine state); the measured triple is the three
warm runs.

| engine | phase | run 1 s | run 2 s | run 3 s | peak RSS MB |
|---|---|---|---|---|---|
| rust | before | 66.19 | 65.51 | 61.53 | 1094 1434 1159 |
| rust | after | | | | |
| sqlite | before | 421.68 | 444.52 | 401.70 | 1309 1245 1299 |
| sqlite | after | | | | |

cold-cache first run, before phase: rust 151.82 s, sqlite 421.68 s.

### failures, triaged

| leg | engine | before | after | bucket |
|---|---|---|---|---|
| `_16_extract_tsi` | rust | red 3/3 | | already red, `CI-KNOWN-RED.md` dl8 battery row |
| `_22_book::probes_compile_as_their_page_says` | rust | red 3/3 | | already red, same file |
| `_15_fold` (both tests) | sqlite | red 3/3 | | already red with the old dylib |
| `_16_extract_tsi` | sqlite | red 3/3 | | already red with the old dylib |
| `_22_book` (many chapters) | sqlite | red 3/3 | | already red with the old dylib |
| `_20_hosts::extract_answers_rows_for_each_family_over_the_corpus` | rust | green | | the `CI-KNOWN-RED` row does not reproduce here |

## R4 the after numbers

Filled after the refit.

## R5 the delta

Filled after the refit.

## R6 `DL8_EVAL_CHECK=1`

Filled after the refit.

## R7 clippy

Filled after the refit.

## R8 the diff

Filled after the refit.