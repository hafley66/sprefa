# V4 Checked Paths

## Purpose

`path``...`` is the first pass of checked string references.

It is for strings that mean "this should resolve to a path under the current run root or as an absolute path". It validates the address and turns it into a cursor. It does not enumerate, read bytes, parse content, or write source location rows.

This keeps the surface small:

```sprf
path`v4/docs/v4-checked-paths.md`
  > read
```

The path check is a gate. If the path exists, one cursor continues. If the path is missing, zero cursors continue and a diagnostic is emitted.

## Current Contract

| Form | Meaning |
| --- | --- |
| `path``file.txt``` | resolve `file.txt` against the run root and require it to exist |
| `` `file.txt` > path `` | use the current cursor value as the path string and require it to exist |
| `path`${FILE}`` | render the path per cursor, then require the rendered path to exist |
| `path``dir``` | allowed; directories pass the check |

Output cursor on success:

| Field | Value |
| --- | --- |
| `cursor.value` | resolved path string |
| `FS` | same resolved path string |
| `cursor_value` | interned string handle when `SprfStore` is present |

Failure:

| Case | Runtime behavior | LSP behavior |
| --- | --- | --- |
| missing path | emit `path/missing`, emit zero cursors | same diagnostic during open/change analysis |
| interpolation with missing runtime value | existing template diagnostics apply first | hover says runtime template |
| empty path string | emits zero cursors without a new diagnostic |

## Separation From Other Ops

`path` validates a string address.

`fs` enumerates files.

`glob` filters path strings.

`read` loads bytes/text from a path cursor or source cursor.

`json`, `ast`, and similar source-aware matchers may read from path/source cursors internally when that is their current contract.

`write_file` and `write_cursor` are mutation ops. `path` does not reserve a write target or create an output file.

## LSP First Pass

Path DSL body hover is implemented through the app hover path:

```text
path
file
/absolute/path/to/file
```

or:

```text
path
missing
/absolute/path/to/missing (No such file or directory ...)
```

Runtime diagnostics produced inside LSP analysis now inherit the source op span when the component did not provide a more precise span. That is why a missing literal path highlights the whole `path``...`` op today.

## Store First Pass

This pass intentionally does not write path existence into the store.

No `_paths`, `_files`, `_where_bytes`, or rule rows are created just because a path exists. Those rows belong to byte-loading or fact-writing steps:

```sprf
path`v4/docs/v4-checked-paths.md`
  > read
  > ...
```

The store still interns the emitted path string when `SprfStore` is present because current cursors can carry `CursorValue::String`.

## Journey

The design pressure came from wanting checked strings without turning every string into a file read. The useful primitive was smaller than `fs`, smaller than `read`, and smaller than a relation. It is a one-row gate over the current cursor value or a path DSL body.

Three constraints shaped the first pass:

- missing path should mean zero cursors, because downstream rules already use empty output as failure
- missing path should also surface in LSP, because checked strings are mostly useful while editing
- `read` remains the byte boundary, because validating a path should not allocate or queue file contents

The implementation followed existing runtime seams:

- `PathComponent` performs the stat gate
- `SpannedComponent` attaches the op span to runtime diagnostics that lack a span
- app DSL hover recognizes `path` bodies without adding a full path DSL module yet
- tests cover runtime rows, runtime diagnostics, LSP hover, and LSP diagnostics

## Current Tests

```bash
cargo test --manifest-path v4/Cargo.toml --test read_gates_bytes_smoke path_
cargo test --manifest-path v4/Cargo.toml --test lsp_hover_smoke path
cargo test --manifest-path v4/Cargo.toml --lib --tests
```

Known full-suite blocker:

```text
cargo test --manifest-path v4/Cargo.toml
```

still compiles examples and currently fails because `v4/examples/dogfood-rust-doc-target.rs` has no `main`.

## Remaining Shape

The next pass can stay narrow:

- add semantic token support for `path` bodies in the VS Code grammar path
- add completion for project-relative paths if it can be bounded and cheap
- decide whether `path` should reject paths outside the run root in LSP mode
- decide whether `path` should have a richer source coord output after the `CursorValue` and `WhereBytes` migration settles
- keep `path?` reserved until there is a real relation/query meaning

