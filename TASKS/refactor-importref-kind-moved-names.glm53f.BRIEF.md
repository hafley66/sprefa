# Brief: ImportRef.kind becomes an enum; moved_names/stem live once on the Rehome seam

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-leaky-types-review.PLAN.md` rows 6 and 20, and PR #507
(`git show 14a1a3678 -- v6/sprefa-extract/src/types.rs`) for the `LangKind`
extension shape you reuse.

## First action
```bash
git merge --ff-only ef5a06239   # STOP AND REPORT on failure
```

## Files you own
- `v6/sprefa-extract/src/types.rs` (`ImportRef` at ~:1968, the new enum, the `Rehome` trait's new default methods only)
- `v6/sprefa-extract/src/move_cx.rs` (one shared `stem` fn)
- `v6/sprefa-extract/src/lang/rust_rehome.rs`, `src/lang/ts_rehome.rs`, `src/lang/prolog/_1_rehome.rs`, `src/lang/kotlin*rehome*.rs` if present
- `src/move_scip.rs:37` only if its `kind` is an `ImportRef` kind (measure; if it is a different struct, leave it)
- `tests/**` as needed; new `tests/7_import_ref_kind.rs`
- new issue: `issuectl new -t improvement --slug importref-kind-moved-names --title "extract move: ImportRef.kind enum, moved_names/stem on the Rehome seam" -a chris -p normal -l extract -l refactor --description "leaky-types review rows 6 and 20"`; tick it as its own commit, after the code commit, never before.
FORBIDDEN: `src/0_move.rs`, `src/move_stage.rs`, `src/lang/*.rs` other than the rehome files, `src/lang/**/_0_source.rs`, `src/scip*.rs`, `src/bin/**`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`.

## Row 6, ImportRef.kind (signatures first)
Measured on ef5a06239: `ImportRef { .. kind: "<str>" }` constructions carry
`"import"` (prolog/_1_rehome.rs:351, ts_rehome.rs:191), `"path_literal"`
(ts_rehome.rs:206), `"manifest_target"` (rust_rehome.rs:200, ts_rehome.rs:106),
`"include"` (rust_rehome.rs:120), `"use_path"` (rust_rehome.rs:138). The one
comparison is `ts_rehome.rs:61` (`reference.kind == "manifest_target"`).
`rust_rehome.rs:814`, `types.rs:715` (`DfLit`) and `move_scip.rs:37` are OTHER
structs; measure before touching, and leave `DfLit` alone.
```rust
// types.rs
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ImportRefKind {
    /// A module import (`use`, `import`, `:- use_module`).
    Import,
    /// A quoted path literal outside an import form.
    PathLiteral,
    /// A package.json / Cargo.toml target line.
    ManifestTarget,
    /// A kind one language owns; tag never equals a core tag (railed).
    Ext(LangKind),
}
impl ImportRefKind { pub const fn as_str(self) -> &'static str }
pub struct ImportRef { /* unchanged */ pub kind: ImportRefKind, /* unchanged */ }
// rust_rehome.rs
pub const INCLUDE: ImportRefKind = ImportRefKind::Ext(LangKind { lang: "rust", tag: "include" });
pub const USE_PATH: ImportRefKind = ImportRefKind::Ext(LangKind { lang: "rust", tag: "use_path" });
```
- `ts_rehome.rs:61` compares `reference.kind == ImportRefKind::ManifestTarget`.
- Every place the old string reached output (JSON rows, `--list` lines, receipts) prints `kind.as_str()`: byte-identical text before and after.
- Receipt: `git grep -n 'kind: "' v6/sprefa-extract/src/lang/*rehome*.rs v6/sprefa-extract/src/lang/prolog/_1_rehome.rs` prints ZERO lines.

## Row 20, moved_names and stem
Measured: `moved_names(cx, &dyn Rehome)` is byte-identical at
`rust_rehome.rs:1533` and `ts_rehome.rs:394` except the index-file name
(`"mod"` vs `"index"`); `stem` is byte-identical at `rust_rehome.rs:1556`,
`ts_rehome.rs:417`, `prolog/_1_rehome.rs:424`.
```rust
// move_cx.rs
/// File name without directory or extension: "src/a/b.rs" -> "b".
pub(crate) fn stem(rel: &str) -> String
// types.rs, on `trait Rehome`, default methods
/// The file name whose stem stands for its directory ("mod" for Rust,
/// "index" for TS). None: no directory-standing file in this language.
fn directory_stem(&self) -> Option<&'static str> { None }
/// Names a move changes in this language's own files: each moved file's stem,
/// plus the directory's stem when the file is the directory-standing one.
fn moved_names(&self, cx: &MoveCx) -> BTreeSet<String> { /* the shared body, via stem + directory_stem */ }
```
- Rust overrides `directory_stem` -> `Some("mod")`, TS -> `Some("index")`; the three private `stem` fns and the two private `moved_names` fns are deleted; call sites use `rehome.moved_names(cx)` / `crate::move_cx::stem`.
- Receipt: `git grep -n 'fn stem\|fn moved_names' v6/sprefa-extract/src` prints exactly `move_cx.rs` once and `types.rs` once.

## Fail-first tests (`tests/7_import_ref_kind.rs`)
1. `import_ref_kind_as_str_is_byte_stable`: core tags `import`, `path_literal`, `manifest_target`, plus rust `include`, `use_path`.
2. `no_ext_import_ref_tag_collides_with_a_core_tag`.
3. `moved_names_uses_the_language_directory_stem`: a Rust move of `src/a/mod.rs` yields `{a, mod}`, a TS move of `src/a/index.ts` yields `{a, index}`, a Prolog move of `src/a.pl` yields `{a}`.
4. `stem_lives_once`: grep rail over `src/**` for `fn stem` and `fn moved_names` counts.
Write each test first, run it, paste the failing line in the commit body, then make it pass.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count pasted.
- Both `git grep` receipts above, verbatim.
- `extract move --list` over one Rust and one TS fixture (`tests/fixtures/**` has both) before and after: `diff` is empty; paste the command.
- `git diff ef5a06239 --stat` shows only owned files; `cargo fmt`; no `eprintln!` in `src/**` beyond the 4 `@eprintln-ok` lines in `bin/extract.rs`; 10-second law.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth. Descriptive identifiers, never single letters. Issue tick as its own commit AFTER the code commit. No `unwrap()` in new non-test code.

## Delivery
One PR against `origin/main`, title `extract move: ImportRef.kind enum, moved_names/stem on the Rehome seam`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test count, both grep receipts>"`.
