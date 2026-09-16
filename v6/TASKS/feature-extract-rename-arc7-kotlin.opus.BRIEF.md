# Brief: extract rename, arc 7: the Kotlin arm over tree-sitter-kotlin-sg

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, receipts :577), the landed Rust arm `v6/sprefa-extract/src/lang/rust_rename.rs` (PR #518) as the reference `Rename` impl, the `Rename` trait at `src/types.rs:2184-2207` and `RenameStop` near `src/types.rs:2143`, `src/lang/kotlin.rs` (`KotlinSource`, :1421; the `type_identifier` / `simple_identifier` seat scans at :138, :160, :208, :234, :294, :452) and `src/lang/kotlin_rehome.rs` for how the Kotlin move arm resolves `package` + `import` (read only). Parser: `tree-sitter-kotlin-sg` already in `Cargo.toml:90`; no new crate.

## First action
```bash
git merge --ff-only e7cd9b883623acbbc19389dd4e2109e4ef1b235b   # STOP AND REPORT on failure
```

## Files you own
- new: `v6/sprefa-extract/src/lang/kotlin_rename.rs`, `tests/7_rename_kotlin.rs`, `tests/fixtures/kotlin_rename/local/{before,after}/` (a two-file source set: `src/main/kotlin/a/Util.kt` with `package a`, `src/main/kotlin/b/Main.kt` with `package b` and `import a.Helper`)
- `src/lang/mod.rs`: ONLY the `pub mod kotlin_rename;` line and adding `&KotlinSource` to the `renames()` roster at :107-109. A concurrent lane adds `&PrologSource` to the same line; the coordinator resolves that one-line conflict.
- new issue: `issuectl new -t feature --slug extract-rename-arc7-kotlin --title "extract rename: arc 7, the Kotlin arm" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md, Kotlin arm, brief shape of arc 5"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs` (a missing `Rename` method or `RenameStop` variant = STOP and hail with the line), `src/0_rename.rs`, `src/1_rename_verify.rs`, `src/rename_cx.rs`, `src/2_move_text.rs`, `src/lang/ts_rename.rs`, `src/lang/rust_rename.rs`, `src/lang/kotlin.rs`, `src/lang/kotlin_rehome.rs`, `src/lang/prolog/**`, every existing `tests/*.rs` and `tests/fixtures/**` other than yours, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`.

## Seats
Definition: the `type_identifier` of `class_declaration` / `object_declaration` / `interface` / `enum` / `typealias`, the `simple_identifier` of a top-level `function_declaration` / `property_declaration`. References: the trailing segment of an `import` (`import a.Helper` -> `Helper`; `import a.Helper as H` -> `Helper` only, `H` stays), `user_type` / `call_expression` / `navigation_expression` identifiers that resolve to the anchor's declaration through the file's own `package` or an explicit import, same-package unqualified use. A wildcard importer `import a.*` that reaches the symbol is a `Dynamic` stop carrying the import span (`kotlin_rehome.rs:120` already finds these). String literals, KDoc, and annotation arguments are never rewritten: report through `text_spellings` for `--text-refs`. A same-named declaration in another package is NOT a seat (test it).

## Fail-first tests (`tests/7_rename_kotlin.rs`)
1. `kotlin_rename_matches_the_hand_written_after`: `before` -> `extract rename src/main/kotlin/a/Util.kt#Helper Tool --commit` -> `diff -rq` vs `after/` = zero entries. The fixture holds: the class def, a companion `Helper()` constructor call in `Main.kt`, a `: Helper` type position, `import a.Helper`, an `import a.Helper as H` in a third file whose body uses `H` (stays), a `"Helper"` string (stays), and `package c` with its own `class Helper` whose uses stay.
2. `wildcard_importer_is_a_dynamic_stop`: a `before` variant with `import a.*` -> exit 6, tree byte-identical.
3. `shadow_in_other_package_needs_no_at`: the `package c` `Helper` never makes the anchor Ambiguous (exit code of the arc 2 table stays 0).
Write each first, paste the failing line in the test header (the `5_rename_rust.rs:5-12` shape), then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND: 0 failures, total count; `7_rename_kotlin` count.
- `grep -n 'panic!\|unwrap()' src/lang/kotlin_rename.rs` = 0 lines.
- `git diff e7cd9b883623acbbc19389dd4e2109e4ef1b235b --stat`: only owned files; `cargo fmt`; no new `eprintln!`; `4_rename_ts` and `5_rename_rust` batteries unchanged.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned. A stop is a `RenameStop`, never a panic.

## Delivery
One PR against `origin/main`, title `extract rename: arc 7, the Kotlin arm`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, diff -rq receipt>"`.
