# extract move: parity with v1-v5 and the port list

Status: research landed 2026-08-27, ports not dispatched. Source: three archive
reads (v1/v2 `~/projects/sprefa-archive-20260428`, v3/v4
`~/projects/sprefa-archive-20260701`, v5 repo root at `54bea3a0e`).

- [Parity table](#parity-table)
- [Port list, ranked](#port-list-ranked)
- [Not ported, and why](#not-ported-and-why)
- [Receipts](#receipts)

## Parity table

| capability | v1 | v2 | v3 | v4 | v5 | v6 now |
|---|---|---|---|---|---|---|
| trigger | you `mv`, watcher rewrites | none (`rewrite_files` is `unimplemented!()`) | byte splice only | byte splice only | `dl --move OLD=NEW --fix` | `extract move OLD NEW`, `--list`, `--commit` |
| languages | JS/TS, Rust | | | | Rust, Kotlin | Prolog, TS, Rust |
| importer rewrite | index-driven, barrels | | | | module-path arithmetic, brace groups | per-language `Rehome` impl |
| Rust strategy | recompute `crate::` path, rewrite `use` + `mod` decl | | | | same, relocate `mod` decl to new parent, private -> `pub(crate)` | keep mod name, add `#[path]`, `use` untouched |
| symbol rename | yes | | | | no | no |
| refs inside moved file | Rust only | | | | Kotlin `package` | yes, incl. `import.meta.url` literals |
| manifests | reads `Cargo.toml` | | | | no | `package.json`, `Cargo.toml` |
| dry run / commit | none | | `--dry-run`, `--approve-only` | dropped | dry default, `--fix` | dry default, `--commit`, one soopy stage |
| verify + rollback | no | | | | `--verify '<cmd>'` | no |
| multi-repo | daemon-wide | | | | `--repo "*"` | no |
| empty dirs, text refs | no | | | | no | yes |
| language dispatch | per crate | | | | two globs, one driver | trait roster, zero switches |

## Port list, ranked

Each row is one lane, one PR, one `Rehome` method or one core flag; none adds a language switch.

| rank | port | from | shape in v6 | receipt |
|---|---|---|---|---|
| 1 | `--verify '<cmd>'`: run checker after commit, roll the soopy stage back on non-zero | v5 `0294e9c2f`, `src/lib.rs:444 run_verify` | core flag; soopy already stages, so rollback = do not commit, or restore pre-run bytes if commit happened first | test: `--verify false` leaves the tree byte-identical; `--verify true` commits |
| 2 | Rust `use`-path re-pathing when a module changes parent (`src/a.rs` -> `src/util/a.rs` with `mod a;` relocated into `util/mod.rs`) | v5 `src/rspath.rs`, `f859585ed` mod surgery; v1 `crates/rs/src/lib.rs:270` | `RustSource::respell` gains a second strategy behind `--relocate-mod` (default stays `#[path]`) | fixture: `use crate::a::f` -> `use crate::util::a::f`, `cargo check` green |
| 3 | private -> `pub(crate)` promotion when a moved item leaves its module's visibility scope | v5 `f859585ed` | Rust impl only | fixture with a private fn used by a sibling |
| 4 | Kotlin `Rehome` | v5 `src/ktpath.rs` | new impl, roster entry; `KotlinSource` exists (`lang/kotlin.rs`) | v5 `tests/it/move_refactor.rs:371,403,464` cases re-cut as fixtures |
| 5 | symbol rename (`extract rename FILE OLD NEW`) | v1 `DeclChange::Rename`, `plan_decl_rename` | new trait method `rename_refs`, or a sibling trait; needs the resolved edge plane (`Resolve<F>`) | out of this plan; write its own plan first |
| 6 | multi-root batch (`--root` repeatable) | v5 `--repo "*"` | core, one `MoveCx` per root | last; no current ask |

## Not ported, and why

- v1 watch-triggered rewrite: the daemon rewriting files on an fs event with no dry run is the failure mode v5 replaced with `--fix`; v6 keeps explicit `--commit`.
- v3 `--approve-only <ids>`: soopy StageRequest is the approval unit now.
- v3/v4 `write_cursor` splices: that is `gen`/soopy Replace, not move.
- v5 `propose_extract`, `sg` codemods: separate verbs, not move parity.

## Receipts

- v5: `src/cli/mod.rs:201-215`, `src/lib.rs:1215 run_move`, `src/lib.rs:1276 move_one_repo`, `src/refactor.rs`, `src/rspath.rs`, `src/ktpath.rs`, `tests/it/move_refactor.rs:39-474`, commits `094655728`, `79db9d9b1`, `f859585ed`, `fc63a0e7c`, `7bf96228f`, `0294e9c2f`.
- v1: `crates/watch/src/plan.rs:104 plan_file_move`, `:107 plan_decl_rename`, `crates/watch/src/js_path.rs:19`, `crates/rs/src/lib.rs:270`.
- v2: `v2/src/writers/_0_mem.rs:111`.
- v3: `crates/pipeline/src/ops/write_cursor.rs`, `crates/server/src/bin/sprefa-run.rs:56-57`.
- v4: `src/v2_ops.rs:3224-3245`.
- v6: PRs #480-#489, `v6/sprefa-extract/src/types.rs` `Rehome`, `src/lang/{ts_rehome.rs,rust_rehome.rs,prolog/_1_rehome.rs}`.
