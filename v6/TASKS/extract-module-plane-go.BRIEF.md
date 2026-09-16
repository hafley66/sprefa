# Lane brief: go module specifiers on the extract call plane

Card: `extract-module-plane-non-ts` (epic `extract-port-closeout`), GO SLICE ONLY.
Branch: `feature/extract-module-plane-go`
Base: origin/main `7f11724b4`.
First action, in your worktree: `git merge --ff-only 7f11724b4`. Failure or a
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

`v6/sprefa-extract` emits module `Specifier` rows from the TypeScript and Rust
front-ends; make the **go** front-end emit them too, from `import`
declarations, on the existing `Specifier` type and the existing closed
`SpecifierKind` vocabulary.

## 2. What already exists (do not rebuild it)

| thing | receipt | note |
|---|---|---|
| `Specifier` type | `v6/sprefa-extract/src/types.rs:489` | `{ span, name: NameId, kind: SpecifierKind, module: Option<NameId> }`. Already exists. Do NOT edit types.rs. |
| what `name` and `module` mean for GO, already decided in writing | `v6/sprefa-extract/src/types.rs:485-492` | "`name` is the specifier text as written (the bound name; **the module path for path-only forms like go's imports**)" and "`None` is for the languages that emit specifiers with the module already in `name` (go's path-only imports)". This is a RECORDED DECISION. Follow it. |
| `SpecifierKind`, closed 5-variant | `v6/sprefa-extract/src/types.rs:528-546` | `Named`, `Default`, `Namespace`, `SideEffect`, `Reexport`, with `as_str()` giving `named`, `default`, `namespace`, `side_effect`, `reexport`. Already exists. Do NOT add a variant. |
| where rows go | `v6/sprefa-extract/src/types.rs:558-567` | `CallFAux.specifiers` |
| the wire record | `v6/sprefa-extract/src/schema.rs:36` | `record=specifier  family=call  span={start,end}  name=<string>  kind=<slug>  module=<string\|null>`. Already exists. Do NOT edit schema.rs. |
| the flatten arm | `src/wire.rs` | already emits specifier rows for every language. Do NOT edit wire.rs. |
| the rust twin, your closest model | `v6/sprefa-extract/src/lang/rust.rs`, the `module specifiers (CallFAux.specifiers)` section | landed in PR #328. Read it for shape, then write the go equivalent against the go grammar. |
| go's call projection entry point | `v6/sprefa-extract/src/lang/go.rs:609-617` `fn project_call` | currently calls `go_walk_call_defs` then `go_walk_call_sites`. You add ONE more call here. |

`grep -n "import" v6/sprefa-extract/src/lang/go.rs` returns ONE hit, a comment
at `:1713`. There is no import walk in the go arm today. You are writing the
first one.

## 3. The grammar, measured

Measured by running the real extractor's cst plane over a real go file. These
are facts, not guesses.

An `import_declaration` contains EITHER one `import_spec` directly (the
single-line `import "fmt"` form) OR one `import_spec_list` whose children are
`import_spec` nodes (the parenthesized block form).

An `import_spec` has:

- an OPTIONAL leading name node, exactly one of:
  - `package_identifier` — an alias, e.g. `alias` in `alias "path/filepath"`
  - `blank_identifier` — the `_` in `_ "embed"`
  - `dot` — the `.` in `. "strings"`
- a REQUIRED `interpreted_string_literal` — the quoted path, quotes INCLUDED
- inside that, an `interpreted_string_literal_content` — the path WITHOUT the
  quotes. **Use this node for the path text. Do not strip quotes by hand.**

Measured node dump for a file containing `import "fmt"` then a block with
`"os"`, `alias "path/filepath"`, `_ "embed"`, `. "strings"`:

```
import_declaration
  import_spec
    interpreted_string_literal
      interpreted_string_literal_content
import_declaration
  import_spec_list
    import_spec
      interpreted_string_literal
        interpreted_string_literal_content
    import_spec
      package_identifier
      interpreted_string_literal
        interpreted_string_literal_content
    import_spec
      blank_identifier
      interpreted_string_literal
        interpreted_string_literal_content
    import_spec
      dot
      interpreted_string_literal
        interpreted_string_literal_content
```

The row's `span` is the **`import_spec` node's own span** (`start_byte` to
`end_byte`), which covers the alias token when present and the closing quote.

## 4. The mapping table you implement

Write this table into the code as the emitter's header comment.

| go source | kind | `name` | `module` |
|---|---|---|---|
| `import "fmt"` | `Named` | `fmt` | `None` |
| `"os"` inside a block | `Named` | `os` | `None` |
| `alias "path/filepath"` | `Named` | `alias` | `Some("path/filepath")` |
| `_ "embed"` | `SideEffect` | `embed` | `None` |
| `. "strings"` | `Namespace` | `strings` | `None` |

The rule in words, so you can code it without re-reading the table:

- **`package_identifier` present** (a real alias): `kind = Named`,
  `name = the alias text`, `module = Some(path)`. This is the ONLY form that
  sets `module` to `Some`, because it is the only form where the path would
  otherwise be lost.
- **`blank_identifier` present**: `kind = SideEffect`, `name = path`,
  `module = None`.
- **`dot` present**: `kind = Namespace`, `name = path`, `module = None`.
- **no name node**: `kind = Named`, `name = path`, `module = None`.

`SpecifierKind::Default` and `SpecifierKind::Reexport` are unreachable from go.
Do not force a use for them.

Receipts for the non-obvious choices:

- The path-only-form rule (`name` carries the path, `module` is `None`) is the
  written decision at `v6/sprefa-extract/src/types.rs:485-492`, which names go
  explicitly. You are following it, not inventing it.
- v5 treats `_` and `.` as distinct import forms and parses them in the same
  slot as an alias: `src/graph/modgraph/go.rs:37-43` (`go_import_single_re`,
  capture group `(_|\.|\w+)`) and `:53-59` (`go_import_line_re`, same group).
- v5's aliased-import binding note is at `src/graph/modgraph/go.rs:10-17`.

## 5. The code to write

Everything lands in `v6/sprefa-extract/src/lang/go.rs`. No other src file changes.

1. Add `Specifier` and `SpecifierKind` to the existing
   `use crate::family::{...}` import block near the top of `go.rs`. Add names to
   the block that is already there; do NOT open a second `use` statement.
2. Write ONE walker plus its helper. Put them right after `fn project_call`.
   Signature and pseudo-code comment first:

```rust
/// Go module specifiers: one row per `import_spec`. Rides the one tree-sitter
/// parse `project_call` already holds. v5 reads the same facts with regexes
/// over stripped text (`src/graph/modgraph/go.rs:37-59`).
fn go_module_specifiers(
    root: tree_sitter::Node,
    src: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
)
// walk the tree for nodes of kind "import_spec"
//   (they sit under import_declaration directly, or under import_spec_list)
// for each: read the optional leading name node kind, read the path from the
//   interpreted_string_literal_content descendant, apply the section-4 rule,
//   collect one row
// push the whole collected Vec into sink.aux.specifiers in ONE extend call
```

3. Call it from `fn project_call` at `v6/sprefa-extract/src/lang/go.rs:609-617`,
   after `go_walk_call_sites`. ONE added line.
4. Use the file's existing helpers rather than new ones: `go_text(node, src)`
   for a node's UTF-8 text (it is already defined in `go.rs`), and build spans
   the way the rest of the file does, `Span { start: node.start_byte() as u32,
   len: (node.end_byte() - node.start_byte()) as u32 }`.
5. Collect into a local `Vec` and push once. A per-row push inside the walk is
   the N+1 shape this repo bans.

## 6. The fixture, verbatim

Create `v6/sprefa-extract/tests/fixtures/go_modules/sample.go` with EXACTLY
these bytes. Do not add, remove or reorder a line; the expected offsets in
section 7 were computed against this exact content, tabs included (the four
lines inside the block start with ONE TAB character, which is what gofmt emits).

```go
// go_modules/sample.go: one line per row of the module-specifier mapping.
// ASCII-only so byte offsets stay simple.

package sample

import "fmt"

import (
	"os"
	alias "path/filepath"
	_ "embed"
	. "strings"
)

func Use(count int) int {
	return count
}
```

The file is 256 bytes. If your file is not 256 bytes (`wc -c`), you typed it
wrong; fix it before writing the test.

## 7. The test, with the exact expected rows

Create `v6/sprefa-extract/tests/25_go_specifiers.rs`, modeled on the existing
`v6/sprefa-extract/tests/24_rust_specifiers.rs` (read that file first and follow
its shape exactly: same header wording, same tuple projection, same assert
style).

The expected rows, in source order. **These offsets were computed by the
coordinator directly from the fixture bytes, independently of the extractor.**

| # | kind | name | module | span.start |
|---|---|---|---|---|
| 1 | `named` | `fmt` | `None` | 142 |
| 2 | `named` | `os` | `None` | 159 |
| 3 | `named` | `alias` | `Some("path/filepath")` | 165 |
| 4 | `side_effect` | `embed` | `None` | 188 |
| 5 | `namespace` | `strings` | `None` | 199 |

**DO NOT EDIT THESE NUMBERS TO MAKE THE TEST PASS.** If the extractor disagrees
with a number, that is a BUG IN YOUR WALKER or a fixture you typed wrong. Fix
the walker or the fixture. If after fixing both you still disagree, STOP and
report the mismatch with the number you got. Changing an expectation to match
output is a failed deliverable.

Also assert one negative: no specifier row has `name == "sample"` (the
`package sample` clause is not an import).

## 8. Files owned / forbidden

OWNED, edit freely:

```
v6/sprefa-extract/src/lang/go.rs
v6/sprefa-extract/tests/25_go_specifiers.rs                (new)
v6/sprefa-extract/tests/fixtures/go_modules/sample.go      (new)
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
v6/sprefa-extract/src/lang/ts.rs, rust.rs, kotlin.rs, python/**
v6/sprefa-extract/Cargo.toml
v6/sprefa-extract/tests/fixtures/go/sample.go              (a graded golden)
everything outside v6/sprefa-extract/
```

`types.rs`, `schema.rs` and `wire.rs` are forbidden BECAUSE this slice needs no
change in them. If you find yourself wanting to edit one, you have left the
slice: STOP and report.

**Never run a bare `cargo fmt`.** The tree is not rustfmt-clean, so a bare run
reformats ~15 files you do not own and your PR gets rejected. If you need to
format, run `rustfmt --edition 2021 <your file>` on YOUR files only.

## 9. Gate and commits

Run from `v6/sprefa-extract` in your worktree. Run the new test THREE times.

```bash
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 25_go_specifiers
cargo test --features cli --test golden_parity
cargo test --features cli --test 4_capability_parity
cargo test --features cli --test 2_df_aux_cli
```

At base `7f11724b4` the suite is **34 binaries, 137 passed, 0 failed**. Any red
is yours. `golden_parity` and `4_capability_parity` MUST stay green: if either
moves, you touched a golden fixture or emitted a node instead of an aux row.

Commits:

1. `extract: go module specifiers from import_spec nodes`
2. `extract: go_modules fixture + 25_go_specifiers`

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
- Colocated consistency: inside `go.rs`, follow that file's existing style.
- Line numbers rot. Verify every citation before relying on it; report any that
  were wrong.

## Report back

Callsign: pick one short readable word, announce it once. Then: commit shas, PR
url, the full `(kind, name, module, span.start)` row table your fixture
produced, the three-run gate output, every brief line number that had rotted
with its real value, and anything you could not do with a `path:line` reason.
