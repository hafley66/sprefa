# extract move: one `Rehome` impl per language

Status: plan, user-decided 2026-08-26. Decision (Chris): "each language as its own impl across the board. never make match arms per lang." Implementation is dispatchable; the trait shape below is the contract a lane implements, and the rust arm is the first new impl.

- [Defect](#defect)
- [Contract](#contract)
- [Lifetimes](#lifetimes)
- [Reads and writes](#reads-and-writes)
- [Arcs](#arcs)
- [Receipts](#receipts)
- [Out of scope](#out-of-scope)

## Defect

`extract move` is a second pipeline beside the extractor. The extractor is plug-and-play (`lang/mod.rs:66 sources()`, `source_for(path)`); move hand-switches on `CorpusLang`:

| site | switch |
|---|---|
| `0_move.rs:219` | `--shim` prolog-only |
| `0_move.rs:241` | `CorpusLang::Prolog` -> `prolog_edits` |
| `0_move.rs:246` | `is_ts` -> `ts_edits` |
| `0_move.rs:380-384` | old/new extension must agree on family |
| `0_move.rs:404` | `is_ts` |
| `0_move.rs:422` | corpus filter `.pl` |
| `1_move_manifest.rs` | hardcoded `package.json` |

It also re-reads the corpus with raw `std::fs` (13 calls, 3 private `read_dir` walks in `0_move.rs`) while `ProjectCx` (`types.rs:1389`) already carries `files`, `manifests`, `reader`, and soopy owns the write side.

## Contract

Type signatures first, pseudo-code as comments.

```rust
// types.rs, beside Source (:1938) and Resolve (:1663)

/// One import-shaped reference the move must respell when its target moves.
pub struct ImportRef {
    pub importer: String,          // project-relative path
    pub literal: Span,             // bytes of the specifier text, quotes excluded
    pub target: String,            // project-relative path it resolves to
    pub kind: &'static str,        // "import" | "path_literal" | "manifest_target" ...
}

/// One respelled literal: what soopy Replace writes.
pub struct Respell {
    pub file: String,
    pub span: Span,
    pub text: String,
}

pub trait Rehome: Source {
    /// Every ImportRef this language owns across `cx.files`, one parse per file.
    /// Reads through cx.reader only. Includes refs INSIDE files about to move.
    fn import_refs(&self, cx: &ProjectCx) -> Vec<ImportRef>;
    //  ts: oxc Specifier rows + ts_paths literals; resolve via oxc_resolver
    //  prolog: use_module/consult/ensure_loaded strings, resolve :785 rule
    //  rust: `mod x;` (+ #[path]), `use crate::a::b`, `include!` literals
    //        resolved through the mod tree; manifest [[bin]]/[lib] path

    /// The literal text for `r` once `moved` is applied (importer AND target
    /// may both have moved). None = unchanged.
    fn respell(&self, r: &ImportRef, moved: &BTreeMap<String, String>) -> Option<Respell>;
    //  ts: relative respell from new importer dir, keep extension policy
    //  prolog: same, drop `.pl`
    //  rust: a `mod` decl for a file moved out of its directory becomes
    //        `#[path = "..."] mod x;`; a `use crate::` path re-spells by the
    //        new mod tree (or is left untouched when the mod name survives)

    /// Manifest carriers this language owns (package.json, Cargo.toml).
    /// Returns ImportRefs with kind "manifest_target" so respell handles them.
    fn manifest_refs(&self, cx: &ProjectCx) -> Vec<ImportRef> { Vec::new() }

    /// A re-export shim left at the old path, when this language has one.
    fn shim(&self, old: &str, new: &str) -> Option<String> { None }
}
```

```rust
// lang/mod.rs
pub fn rehomes() -> &'static [&'static dyn Rehome];   // roster, same order law as sources()
pub fn rehome_for(path: &str) -> Option<&'static dyn Rehome>;
```

```rust
// 0_move.rs, language-free
// plan(cli):
//   cx      = ProjectCx::open(root)          // files, manifests, reader; one walk
//   moved   = validated_moves(cx, tsv)       // each old must have rehome_for(old) == rehome_for(new)
//   refs    = rehomes().flat_map(|r| r.import_refs(&cx) ++ r.manifest_refs(&cx))
//   edits   = refs.filter_map(|ref| rehome_for(ref.importer).respell(ref, moved))
//   shims   = if cli.shim { moved.map(|(o,n)| rehome_for(o).shim(o,n)) }
//   stages  = [Replace(edits) sorted by file, Move(moved), Create(shims)]
//   commit  -> soopy StageRequest, then rmdir sweep (#484), then --text-refs (#487)
```

`CorpusLang` and `is_ts` leave `0_move.rs`. `1_move_manifest.rs` folds into `TsSource::manifest_refs`. `2_move_text.rs` stays language-free (it is text, not a language).

## Lifetimes

| type | lives |
|---|---|
| `ProjectCx` | one per `extract move` invocation; built once, borrowed by every impl |
| `&'static dyn Rehome` | process-static roster, zero state, like `sources()` |
| `Vec<ImportRef>` | one plan; dropped after edits are built |
| soopy `StageRequest` | one per run, as today |

## Reads and writes

| step | reads | writes |
|---|---|---|
| open | `ProjectCx` walk (soopy `_7_source_tree.rs:59 snapshot`) | none |
| import_refs | `cx.reader`, one parse per file per language | none |
| respell | in-memory | none |
| commit | none | one soopy StageRequest; `remove_dir` sweep |

Uniqueness: `(file, span)` is unique across all `Respell`s; two impls claiming one span is a plan error (assert, name both impls). One `Rehome` per path (first-match roster).

## Arcs

| arc | owns | receipt |
|---|---|---|
| 1 trait + roster + language-free core; ts and prolog impls moved into `lang/ts.rs`, `lang/prolog/_0_source.rs` | `types.rs`, `lang/mod.rs`, `0_move.rs`, `1_move_manifest.rs` (deleted) | `tests/1_move.rs` 12 + `2_move_refs` 4 unchanged; Grapht 66-move batch diff stays 7; `grep -c 'CorpusLang::' 0_move.rs` = 0; `grep -c 'std::fs' 0_move.rs` <= 2 (rmdir sweep) |
| 2 rust impl | `lang/rust.rs`, new `tests/3_move_rust.rs` | fail-first: move `src/lang/ts_paths.rs` -> `src/lang/ts/paths.rs` inside a copy of `sprefa-extract` itself, `cargo check` green after; `Cargo.toml` `[[bin]] path` case |
| 3 `--shim` through `Rehome::shim` | `0_move.rs` | prolog shim test unchanged; ts/rust shim = None with a named error |

Sequence 1 -> 2; 3 rides with 1.

## Receipts

- `git grep -n 'CorpusLang::\|is_ts(' v6/sprefa-extract/src/0_move.rs` prints nothing.
- `git grep -n 'read_dir' v6/sprefa-extract/src/0_move.rs v6/sprefa-extract/src/lang/ts_walk.rs` prints nothing.
- Grapht batch on `f427e81`: diff vs `00005e2` = 7, same 7 files as #487's report.
- rust self-move above; `cargo test --features cli` 0 failures.

## Out of scope

- SCIP as an `import_refs` source: `ScipSource` (`types.rs:1887`) gives symbol occurrences, not the literal span and quoting a Replace needs; a later impl may use it to *verify* edges. Named, not built.
- Rewriting text carriers (`--text-refs` stays report-only, plan `2026-08-25-extract-move-typescript.PLAN.md:642-644`).
- Go, Kotlin, Python, Markdown impls: the trait admits them; none requested.
