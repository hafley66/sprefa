---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: normal
epic: extract-move-parity
labels: [extract, refactor, kotlin]
---

# extract move: Kotlin Rehome impl

## Description

v5 src/ktpath.rs. New impl + roster entry on KotlinSource (lang/kotlin.rs). Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: v5 tests/it/move_refactor.rs:371,403,464 cases re-cut as fixtures

## Acceptance Criteria

- [x] `impl Rehome for KotlinSource` lives in its own file, `v6/sprefa-extract/src/lang/kotlin_rehome.rs`, with one roster line in `lang/mod.rs:92`.
- [x] Zero `match`/`if` on language in `src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`; those three files are unchanged by this arc.
- [x] `import_refs` emits kind `"import"` for every explicit `import a.b.Decl` whose target file is in the batch, and kind `"package_decl"` for the moved file's own `package` declaration.
- [x] The import walk is `kotlin.rs`'s own `kt_walk_import_headers`, reused through a visibility widening; no regex, no second parse shape.
- [x] The source root is derived from the old path minus the declared package dirs; a layout/package disagreement is a named error that rewrites nothing.
- [x] Wildcard imports (`import a.b.*`) and same-package bare uses are counted into `warn` lines and never rewritten.
- [x] `manifests`, `manifest_refs`, `shim` and `text_spellings` stay at their trait defaults, each with a stated reason.
- [x] `tests/4_move_kotlin.rs` carries the five fail-first cases over `tests/fixtures/kotlin_move/**`.

## Agent Runs

### 2026-08-27 · @feature-move-kotlin-rehome

Rank 4 of the extract-move-parity epic, on `afa481059`.

- New: `v6/sprefa-extract/src/lang/kotlin_rehome.rs`, `tests/4_move_kotlin.rs`, `tests/fixtures/kotlin_move/{basic,mismatch}/**`.
- Edited: `lang/mod.rs` (`pub mod kotlin_rehome;` + the roster entry), `lang/kotlin.rs` (`kt_parse`, `kt_text`, `kt_first_child`, `kt_child_kind`, `kt_walk_import_headers` become `pub(crate)`; nothing else moves).
- The respell span is the dotted path ALONE (`identifier.start` + the interned module's byte length), never the `import_header` node, which runs on through an `as` alias, a `.*` and the line terminator. `import com.lib.Util as U` comes out `import com.core.Util as U`.
- `warn` and `error` lines print to stdout beside the plan table. They lead it rather than follow it: they are read off during `Plan::build`, which runs before `0_move.rs:64` prints `root`.
- Deviation from v5 `ktpath.rs:42-44`: a moved file with no top-level decls is NOT an error. v5's bail skipped the `package` rewrite too, which leaves a moved file declaring a package it no longer sits in. Here the package line follows the file and there are simply no import respells.
- New named error v5 has no equivalent for: a destination at the source root lands in the default package, whose decls stop being importable.
