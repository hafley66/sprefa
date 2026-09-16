# Lane brief: kotlin module specifiers on the extract call plane

Card: `extract-module-plane-non-ts` (epic `extract-port-closeout`), KOTLIN
SLICE ONLY. The go slice landed as PR #338; this is its kotlin twin.
Branch: `feature/extract-module-plane-kotlin`
Base: origin/main `4e0767f6b`.
First action, in your worktree: `git merge --ff-only 4e0767f6b`. Failure or a
missing tree = STOP AND REPORT, never work around it.

This brief is MECHANICAL. Every node kind, every mapping row and every expected
byte offset below was measured by the coordinator against the real grammar and
the real fixture bytes. Implement exactly what is written. Do not improvise, do
not redesign, do not widen any enum.

## TOC

1. The job in one sentence
2. What already exists (do not rebuild it)
3. The grammar, measured
4. The mapping table you implement
5. The code to write
6. The fixture, verbatim
7. The test, with the exact expected rows
8. Files owned / forbidden
9. Gate and commits
10. Style laws

---

## 1. The job in one sentence

`v6/sprefa-extract` emits module `Specifier` rows from the TypeScript, Rust and
Go front-ends; make the **kotlin** front-end emit them too, from `import`
headers, on the existing `Specifier` type and the existing closed
`SpecifierKind` vocabulary.

## 2. What already exists (do not rebuild it)

| thing | receipt | note |
|---|---|---|
| `Specifier` type | `v6/sprefa-extract/src/types.rs:489` | `{ span, name: NameId, kind: SpecifierKind, module: Option<NameId> }`. Do NOT edit types.rs. |
| `SpecifierKind`, closed 5-variant | `v6/sprefa-extract/src/types.rs:528-546` | `Named`, `Default`, `Namespace`, `SideEffect`, `Reexport`; `as_str()` gives `named`, `default`, `namespace`, `side_effect`, `reexport`. Do NOT add a variant. |
| where rows go | `v6/sprefa-extract/src/types.rs:558-567` | `CallFAux.specifiers` |
| the wire record | `v6/sprefa-extract/src/schema.rs:36` | already emits for every language. Do NOT edit schema.rs or wire.rs. |
| **the go twin, your model, landed 4 hours ago** | `v6/sprefa-extract/src/lang/go.rs`, section `module specifiers (CallFAux.specifiers)` right after `fn project_call` | `go_module_specifiers` + `go_walk_import_specs` + two tiny helpers. READ IT FIRST and mirror its shape exactly: same header-comment table, same recursive walk, same collect-then-one-extend. |
| the rust twin, for the symbol-level `module` convention | `v6/sprefa-extract/src/lang/rust.rs`, same section name | Kotlin imports are symbol-level like rust's `use`, so the `module` column follows RUST (full path as written), NOT go (path-only, `module = None`). |
| kotlin's call projection entry point | `v6/sprefa-extract/src/lang/kotlin.rs:587-595` `fn project_call` | currently calls `kt_walk_call_defs` then `kt_walk_call_sites`. You add ONE more call here. |
| kotlin's text helper | `v6/sprefa-extract/src/lang/kotlin.rs:65` `fn kt_text(node, src) -> &str` | use it, do not write another. |
| kotlin's span helper | `v6/sprefa-extract/src/lang/kotlin.rs:70` `fn node_span(node) -> Span` | use it for the import span, do not hand-build a `Span`. |

`grep -n "import" v6/sprefa-extract/src/lang/kotlin.rs` returns nothing today.
There is no import walk in the kotlin arm. You are writing the first one.

## 3. The grammar, measured

Measured by running the real extractor's cst plane over the real fixture in
section 6. These are facts, not guesses.

The `source_file` contains one `import_list` holding one `import_header` per
import line. An `import_header` has:

- the `import` keyword (anonymous, no named node)
- a REQUIRED `identifier` node covering the WHOLE dotted path as written,
  e.g. `kotlin.collections.List` or `java.util.Map` or `kotlin.text`. For the
  wildcard form the `identifier` STOPS BEFORE the `.*`; the trailing `.` is an
  anonymous token and the `*` is its own node.
- then EXACTLY ONE OF, or neither:
  - an `import_alias` node covering `as JMap`, whose named child is a
    `type_identifier` covering just `JMap`
  - a `wildcard_import` node covering just `*`

Measured node dump for the section-6 fixture (byte offsets):

```
import_header   139..169     import kotlin.collections.List
  identifier    146..169     kotlin.collections.List
import_header   170..198     import java.util.Map as JMap
  identifier    177..190     java.util.Map
  import_alias  191..198     as JMap
    type_identifier 194..198 JMap
import_header   199..219     import kotlin.text.*
  identifier    206..217     kotlin.text
  wildcard_import 218..219   *
```

The row's `span` runs from the **`identifier` node's start** to the
**`import_header` node's end**. That covers the path, the alias when present,
and the `.*` when present, and excludes the `import` keyword, the same shape as
go's `import_spec` span. Build it as
`Span { start: identifier.start_byte() as u32, len: (header.end_byte() - identifier.start_byte()) as u32 }`
(you may not use `node_span` for this one because it spans two nodes; every
other span in the file uses `node_span`).

## 4. The mapping table you implement

Write this table into the code as the emitter's header comment.

| kotlin source | kind | `name` | `module` |
|---|---|---|---|
| `import kotlin.collections.List` | `Named` | `List` | `Some("kotlin.collections.List")` |
| `import java.util.Map as JMap` | `Named` | `JMap` | `Some("java.util.Map")` |
| `import kotlin.text.*` | `Namespace` | `text` | `Some("kotlin.text")` |

The rule in words, so you can code it without re-reading the table:

- **`import_alias` present**: `kind = Named`, `name = the type_identifier text`
  (the alias), `module = the identifier text` (the full path).
- **`wildcard_import` present**: `kind = Namespace`, `name = the LAST dotted
  segment of the identifier text` (`text` for `kotlin.text`), `module = the
  identifier text` (the package path, WITHOUT `.*`).
- **neither**: `kind = Named`, `name = the LAST dotted segment of the identifier
  text` (`List` for `kotlin.collections.List`), `module = the identifier text`
  (the full path).

`module` is ALWAYS `Some` for kotlin. `Default`, `SideEffect` and `Reexport` are
unreachable from kotlin. Do not force a use for them.

Receipts for the non-obvious choices:

- The full-path-in-`module` convention is the RUST decision, landed in PR #328,
  header table at `v6/sprefa-extract/src/lang/rust.rs` section `module
  specifiers`: `use a::b;` gives `name=b, module=a::b`; `use a::b as c;` gives
  `name=c, module=a::b`; `use a::*;` gives `Namespace, name=a, module=a`. Kotlin
  is the same symbol-level import shape, so it takes the same convention. Go
  differs because a go import binds a whole package, never a symbol.
- Last-segment as the bound name and alias-overrides-it is v5's kotlin rule,
  `src/graph/modgraph/kotlin.rs:203-207` (`source = spec.rsplit('.').next()`,
  `local = alias.unwrap_or(source)`).
- Wildcard as `Namespace` mirrors rust `use a::*` and v5's wildcard branch at
  `src/graph/modgraph/kotlin.rs:164-176`.
- Backticked identifiers (`` import a.`weird name` ``) are OUT OF SCOPE for this
  slice: emit the text as written, do not strip backticks. Do not add a test
  for them.

## 5. The code to write

Everything lands in `v6/sprefa-extract/src/lang/kotlin.rs`. No other src file
changes.

1. Add `Specifier` and `SpecifierKind` to the existing `use crate::family::{...}`
   block at `kotlin.rs:40-44`. Add names to the block that is already there; do
   NOT open a second `use` statement. Keep the block rustfmt-shaped (go.rs shows
   the resulting wrap).
2. Write ONE walker plus helpers, placed right after `fn project_call`, in the
   same section-comment style go.rs uses. Signature and pseudo-code first:

```rust
/// Kotlin module specifiers: one row per `import_header`. Rides the one
/// tree-sitter parse `project_call` already holds. v5 reads the same facts
/// with a regex over stripped text (`src/graph/modgraph/kotlin.rs:19-26`).
fn kt_module_specifiers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
)
// walk the tree for nodes of kind "import_header"
// for each: find the child `identifier` (path text), the optional child
//   `import_alias` (alias text = its `type_identifier` child), the optional
//   child `wildcard_import`; apply the section-4 rule; collect one row
// push the whole collected Vec into sink.aux.specifiers in ONE extend call
```

   The pseudo-code above is for YOU. Do NOT paste it into the function body;
   the coordinator stripped exactly that from the go PR before merging.
3. Call it from `fn project_call` at `kotlin.rs:587-595`, after
   `kt_walk_call_sites`. ONE added line.
4. Use `kt_text` for text. Collect into a local `Vec`, push once. A per-row push
   inside the walk is the N+1 shape this repo bans.
5. Do NOT touch any other line of `kotlin.rs`. If `rustfmt` reflows an
   unrelated hunk, revert that hunk before committing; the go PR had one such
   stray hunk and the coordinator reverted it.

## 6. The fixture, verbatim

Create `v6/sprefa-extract/tests/fixtures/kotlin_modules/sample.kt` with EXACTLY
these bytes. Do not add, remove or reorder a line; the expected offsets were
computed against this exact content.

```kotlin
// kotlin_modules/sample.kt: one line per row of the module-specifier mapping.
// ASCII-only so byte offsets stay simple.

package sample

import kotlin.collections.List
import java.util.Map as JMap
import kotlin.text.*

fun use(count: Int): Int = count
```

The file is 254 bytes. If your file is not 254 bytes (`wc -c`), you typed it
wrong; fix it before writing the test.

## 7. The test, with the exact expected rows

Create `v6/sprefa-extract/tests/26_kotlin_specifiers.rs`, modeled on
`v6/sprefa-extract/tests/25_go_specifiers.rs` (read that file first and follow
its shape exactly: same header wording, same tuple projection, same assert
style; the driver is `KotlinSource`, exported from the crate root the same way
`GoSource` is).

The expected rows, in source order. **These offsets were computed by the
coordinator directly from the fixture bytes, independently of the extractor.**

| # | kind | name | module | span.start |
|---|---|---|---|---|
| 1 | `named` | `List` | `Some("kotlin.collections.List")` | 146 |
| 2 | `named` | `JMap` | `Some("java.util.Map")` | 177 |
| 3 | `namespace` | `text` | `Some("kotlin.text")` | 206 |

**DO NOT EDIT THESE NUMBERS TO MAKE THE TEST PASS.** If the extractor disagrees
with a number, that is a BUG IN YOUR WALKER or a fixture you typed wrong. Fix
the walker or the fixture. If after fixing both you still disagree, STOP and
report the mismatch with the number you got. Changing an expectation to match
output is a failed deliverable.

Also assert one negative: no specifier row has `name == "sample"` (the
`package sample` header is not an import).

## 8. Files owned / forbidden

OWNED, edit freely:

```
v6/sprefa-extract/src/lang/kotlin.rs
v6/sprefa-extract/tests/26_kotlin_specifiers.rs               (new)
v6/sprefa-extract/tests/fixtures/kotlin_modules/sample.kt     (new)
```

FORBIDDEN, do not edit; other drivers and lanes own them:

```
~/projects/hafley-rs                                       (entire repo)
v6/sprefa-extract/src/types.rs
v6/sprefa-extract/src/schema.rs
v6/sprefa-extract/src/wire.rs
v6/sprefa-extract/src/deps.rs
v6/sprefa-extract/src/project.rs
v6/sprefa-extract/src/dispatch.rs
v6/sprefa-extract/src/lang/mod.rs
v6/sprefa-extract/src/lang/ts.rs, rust.rs, go.rs, python/**
v6/sprefa-extract/Cargo.toml
v6/sprefa-extract/tests/fixtures/kotlin/**                 (graded goldens)
everything outside v6/sprefa-extract/
```

`types.rs`, `schema.rs` and `wire.rs` are forbidden BECAUSE this slice needs no
change in them. If you find yourself wanting to edit one, you have left the
slice: STOP and report.

**Never run a bare `cargo fmt`.** The tree is not rustfmt-clean, so a bare run
reformats ~15 files you do not own and your PR gets rejected. If you need to
format, run `rustfmt --edition 2021 <your file>` on YOUR files only, then check
`git diff` for stray hunks and revert them.

## 9. Gate and commits

Run from `v6/sprefa-extract` in your worktree. Run the new test THREE times.

```bash
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 26_kotlin_specifiers
cargo test --features cli --test golden_parity
cargo test --features cli --test 4_capability_parity
cargo test --features cli --test 2_df_aux_cli
```

At base `4e0767f6b` the suite is **36 binaries, 144 passed, 0 failed**. Any red
is yours. `golden_parity` and `4_capability_parity` MUST stay green: if either
moves, you touched a golden fixture or emitted a node instead of an aux row.

Commits:

1. `extract: kotlin module specifiers from import_header nodes`
2. `extract: kotlin_modules fixture + 26_kotlin_specifiers`

Push the branch and open a PR against `main` with the row table and the
three-run gate output. Do NOT merge it yourself and do NOT push `main`.

## 10. Style laws (repo, non-negotiable)

- **No em dashes** anywhere, prose or comments.
- **Comment budget**: comments state only constraints the code cannot show. No
  change-log narrative, no dates, no arc references, no restating the next line.
- Banned words in prose AND identifiers: `provenance`, `substrate`,
  `load-bearing`, `regime`, `refusal`.
- No negative parallelism (`not X, Y`; `this isn't X. it's Y`).
- `tracing` only. **No `eprintln!` in `src/**`.**
- **Descriptive variable names, never single-letter.**
- **N+1 forbidden**: collect the set, one push.
- Colocated consistency: inside `kotlin.rs`, follow that file's existing style.
- Line numbers rot. Verify every citation before relying on it; report any that
  were wrong.

## Report back

Callsign: pick one short readable word, announce it once. Then: commit shas, PR
url, the full `(kind, name, module, span.start)` row table your fixture
produced, the three-run gate output, every brief line number that had rotted
with its real value, and anything you could not do with a `path:line` reason.
