# rust excess 1: the name-pick class (ours minus oracle, after the projection)

From `plans/extract-bench-2026-08-29/RUST-PARITY.REPORT.md` section 6: after
the scope + closure projection, 50/300 (vs ra) and 25/300 (vs scip) of the
sampled excess rows resolve a callee whose def does not exist in the dst file
we name. These are bare-name picks where the real callee is a tuple-variant
ctor, a type name in value position, or a def living in another file.

## Ask

Classify the name-pick rows (about 50/300 x 24,225 excess = 4,000 rows vs ra;
1,400 vs scip) into ctor / type-name-in-value-position / wrong-def, then bind
the ctor class through the corpus def table (variant and tuple-struct ctors
already have spans from the type parse) and drop type-name rows to
`no_corpus_def`. Expected reclaim: 1-2 points of ra recall, worth taking only
if the ctor leg stays under 100 lines in `src/lang/rust.rs` +
`rust_receivers.rs`.

## Receipt so far

| example | site | what it is |
|---|---|---|
| `Definition` | `crates/ide/src/rename.rs:338` `NameClass::Definition(it)` pattern binding, dst `crates/ide-db/src/defs.rs` | enum variant in pattern position, a type-name pick |
| `MalformedDerive` | `crates/hir/src/diagnostics.rs` | same shape |
| `AttributeTemplate` | `crates/hir-expand/src/inert_attr_macro.rs:18` | struct name in type position |

## Owner note

Fix lane owns `src/lang/rust.rs`, `src/lang/rust_receivers.rs`,
`tests/7N_rust_*.rs`. Fail-first with a fixture under
`tests/fixtures/rust_findings/` per repo convention.
