# Brief: extract move, `impl Rehome for RustSource` (arc 2)

Read `CLAUDE.md` and `AGENTS.md` in full first. Then read
`plans/2026-08-26-extract-move-rehome-trait.PLAN.md` and its
`.visual.human.unga.md` (both on origin/main), and PR #488's body
(`gh pr view 488`) for the deviations that landed: `MoveCx` (not
`ProjectCx`), `ImportRef` carries literal text, `import_refs` is
batch-gated, `manifests()` / `text_spellings()` exist.

User decision (Chris, 2026-08-26): every language is its own impl. No
`match`/`if` on language anywhere in the move core. You add ONE impl and
ONE roster entry; you do not touch the core.

## First action

```bash
git merge --ff-only d775bfa13   # STOP AND REPORT on failure
```

## Where things are

- Trait: `v6/sprefa-extract/src/types.rs` `pub trait Rehome` (`import_refs`,
  `respell`, `manifests`, `manifest_refs`, `shim`, `text_spellings`).
- Roster: `src/lang/mod.rs:89 rehomes()` = `[&PrologSource, &TsSource]`;
  `rehome_for` picks by `source_for(path).name()`. Add `&RustSource`.
- Reference impls: `src/lang/ts_rehome.rs` (835 lines), `src/lang/prolog/_1_rehome.rs`.
  Match their file shape: new `src/lang/rust_rehome.rs` (or `rust/_1_rehome.rs`
  if you split; do not reorganise `rust.rs`).
- Existing syn walk in `src/lang/rust.rs`: `mod` items and `#[path]` at
  `:1260-1265`, `mod_path_attr` `:1404`, `UseTree` fold `:1327-1353`.
  Reuse these; never regex Rust source. syn is the parser, per the user
  law "syn for rust, oxc for ts".
- Core: `src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`. FORBIDDEN
  to edit except a one-line roster entry in `lang/mod.rs` and `lib.rs`/`mod.rs`
  wiring for the new file. If the core needs a change to admit Rust, STOP
  and hail with the exact line and why.

## What the Rust impl answers

| question | answer |
|---|---|
| import_refs | every `mod x;` declaration (file-level and inline, resolving `x.rs` / `x/mod.rs` / `#[path]` from the declaring file's directory per rustc rules), `include!`/`include_str!`/`include_bytes!` string literals, and `#[path = ".."]` literals. Kind names: `"mod_decl"`, `"path_attr"`, `"include"`. Also `use crate::a::b` / `use super::` / `use self::` paths as kind `"use_path"` with their resolved target file, when the target is a module file the batch moves. |
| respell | `mod_decl` whose target file leaves its rustc-derivable location: emit `#[path = "<relative from declaring file dir>"]` above the decl (Insert), or rewrite an existing `#[path]` literal. A move that keeps the file where rustc would look (e.g. `x.rs` -> `x/mod.rs`) = None. `use_path`: unchanged when the module NAME survives (the mod tree, not the fs path, names it); when the mod declaration itself is rehomed to a different parent module, respell the leading segments. `include`: relative respell from the including file's directory. |
| manifests | `Cargo.toml` files under the root (workspace and members). |
| manifest_refs | `[lib] path`, `[[bin]] path`, `[[test]] path`, `[[bench]] path`, `[[example]] path`, `build = "..."`, workspace `members` entries that name a moved directory. Edit with `toml_edit` (preserves formatting; research alternatives and state the one-line reason in the PR body). |
| shim | None with a named error `"rust has no shim form"` (a `pub use` re-export shim is possible; NOT built; say so in a comment with the throw site). |
| text_spellings | `target/` compiled paths are not stable; return empty. |

Old/new extension law: `.rs` -> `.rs` only (`validated_moves` handles it via
`rehome_for`; confirm, do not add a switch).

## Fail-first tests, `tests/3_move_rust.rs`, fixture `tests/fixtures/rust_move/`

Each test fails before the impl and passes after; state the assertion line
that failed in the PR body.

1. `a_mod_decl_gains_a_path_attr_when_its_file_leaves_its_dir`: `src/a.rs` -> `src/util/a.rs`, `src/lib.rs` gets `#[path = "util/a.rs"] mod a;`.
2. `a_move_to_the_mod_rs_form_changes_nothing`: `src/a.rs` -> `src/a/mod.rs`, zero Respells.
3. `an_existing_path_attr_is_respelled`.
4. `an_include_str_literal_is_respelled`.
5. `a_use_path_survives_when_the_mod_name_survives` (zero Respells on `use crate::a::f`).
6. `cargo_toml_bin_path_follows_the_move`: `[[bin]] path = "src/bin/x.rs"` -> moved.
7. `dry_run_prints_every_respell_and_touches_nothing`.
8. Self-move oracle: copy `v6/sprefa-extract` into a temp dir, `extract move src/lang/ts_rehome.rs src/lang/ts/rehome.rs --commit`, then `cargo check` in the copy passes. Cap 10 s per test; if cargo check exceeds it, mark `#[ignore]` with the measured time and run it once by hand for the PR body.

## Receipts (PR body)

- `cargo test -p sprefa-extract --features cli`: full battery 0 failures; `3_move_rust` count; `1_move` 12 and `2_move_refs` 4 unchanged.
- `git grep -n 'CorpusLang::\|ExtractLang::\|match .*lang' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` prints nothing.
- Diff of the core files is the roster line only: `git diff d775bfa13 -- v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/move_cx.rs v6/sprefa-extract/src/move_stage.rs` empty.
- Grapht 66-move oracle (ts) unchanged at 9: fresh detached worktrees of `~/projects/hafley-rxjs` at `f427e81` and `00005e2` under `~/projects/hafley-rxjs/.boop-worktrees/` ONLY, removed after; never touch existing worktrees or branches.
- `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test.

## Style

Comment budget: constraints only. Banned words: provenance, substrate,
load-bearing, regime, refusal, ground truth. Descriptive identifiers.
Build-vs-buy: name the TOML editing crate you picked and the one you rejected.

## Delivery

One PR against `origin/main`, title `extract move: Rehome for Rust (arc 2)`.
Hail on post and on block:
`boop beep hail sprefa-coordinator --from <your-lane> --body "<PR#, test counts, self-move result>"`.
Do not merge.
