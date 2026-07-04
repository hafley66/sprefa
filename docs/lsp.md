# LSP mode: declarative diagnostics

`dl --lsp <program.dl> --root <dir>` runs the engine as a Language Server. Any
`.dl` program that declares a relation named `diag` becomes a live linter: edit a
file, the engine ticks the changed path, and the rows in `diag` become squiggles
in the editor (and in any LSP client, including Claude Code's IDE bridge).

No diagnostic code lives in the `.dl`. You write a rule whose head is `diag`. The
engine maps the columns by NAME, so column order is free.

## The `diag` convention

```
rel diag(path: file, line: int, col: int, end_line: int, end_col: int,
         severity: text, msg: text).
```

| column | required | default | meaning |
|---|---|---|---|
| `path` | yes | — | file the diagnostic attaches to (a `file`/`path` col) |
| `line` | yes | — | 1-based start line |
| `msg` | yes | — | the message shown |
| `col` | no | 0 | 0-based start column; 0 ⇒ whole-line |
| `end_line` | no | `line` | 1-based end line |
| `end_col` | no | line end | 0-based end column |
| `severity` | no | `"warn"` | `error` \| `warn` \| `info` \| `hint` |

Only `path`, `line`, `msg` are needed for a working squiggle. Without `col`/`end_col`
the whole line is underlined. For a tight squiggle, bind the span off the match:
`sg(path, rev, :lang, "pat", line, col, end_line, end_col)` returns 1-based lines
and 0-based byte columns (== char/UTF-16 for ASCII source), which the `diag` rule
forwards to the `col`/`end_col` columns. See `examples/lint-unwrap.dl`.

## The `def_target` convention (go-to-def)

A program-declared rel that drives `textDocument/definition`. When the engine
sees `def_target` declared, it queries it by the bare text under the cursor and
returns each `(file, line)` as a jump target. Falls back to module-edge
resolution (import specifiers) when the rel is absent or the name has no row.

```
rel def_target(name: text, file: file, line: int, kind: text).
def_target(name, f, l, "type") <- type_entity(_, name, k, _, f, l), k =~ /struct|enum|trait|alias/.
def_target(name, f, l, "fn")   <- type_entity(_, name, k, _, f, l), k =~ /function|method/.
```

| column | required | meaning |
|---|---|---|
| `name` | yes | the bare text under the cursor (matched by equality) |
| `file` | yes | the definition file (1-based line lands at the real def) |
| `line` | yes | 1-based definition line |
| `kind` | no | carried for future use (decorations); not required for the jump |

Multiple rows for the same name (overloads, distinct modules) produce multiple
locations; the editor shows a picker. Without `def_target`, go-to-def resolves
only import specifiers via `module_edge`.

## The hover handler

`textDocument/hover` auto-synthesizes a markdown summary from the type and call
graphs — no rel to declare. The cursor's located span resolves to bare text,
then the engine joins:

- `type_entity(_, name, kind, parent, file, line)` — entities by bare name
- `call_name(sym, name)` -> `call_def(sym, kind, file, line, _)` — callables

Each match renders as one markdown block (`**kind** \`sym\`` + `file:line`),
separated by `---`. The program opts in by referencing `type_entity` or
`call_def` (the lazy indexers populate those tables only when referenced).

## Muting a diagnostic code (session-scoped)

Run `dl: Toggle Diagnostic Code` (bound to `cmd+alt+d cmd+alt+d` / `ctrl+alt+d
ctrl+alt+d`) — a quick-pick of the codes currently in `diag`, each with a muted
checkmark. Toggling one flips it in the engine's `diag_mute` set and instantly
republishes with muted codes filtered out (the `dl.toggleDiagCode` /
`dl.listDiagCodes` `workspace/executeCommand` pair). The first user is the
`std/suppress.dl` `dl-directive` info dots.

Muting is session/db-scoped — the mute row persists in the db, so it survives a
daemon restart — and it is filtered at the LSP publish seam only. `--check` /
`--parse-only` read the `diag` relation directly and are never affected: a mute
is an editor affordance, never a CI gate.

## How it ticks

| event | action |
|---|---|
| `initialize` | open db + parse program; full `tick` once; publish diagnostics for every file with `diag` rows |
| `didOpen` / `didSave` | `tick_paths` the one changed path (disk truth), re-query `diag` for it, publish |
| `didChange` | ignored in v1 — the engine reads disk, and unsaved-buffer support is the RAM-only level of the data model (deferred). Lint fires on save |

Save-driven, deterministic, disk-truth. This is the "rock beats AI" posture: the
rule either matches or it does not, and the same edit always yields the same
squiggle.

## Running it

```
cargo build --release
./target/release/dl examples/lint-unwrap.dl --root /path/to/repo --lsp
```

The server speaks LSP over stdio. It logs one line to stderr on startup
(`[lsp] ready: N diagnostic(s) ...`); everything else on stdout is protocol.

## Editor glue (VSCode)

There is no bespoke extension yet. Drive it through any generic
"start an LSP server for this language" client (for example the `glspc` /
"generic-lsp" style extensions), pointed at the binary:

| setting | value |
|---|---|
| server command | `/abs/path/to/dl` |
| server args | `["examples/lint-unwrap.dl", "--root", "${workspaceFolder}", "--lsp"]` |
| language id | `rust` (or whatever the rule scans) |

A bespoke `dl-lsp` VSCode extension (auto-start, status bar, multi-program) is
the natural follow-on once the rule set grows; a flag-driven generic client is
enough to see squiggles today.

## Claude Code

Claude Code's IDE bridge consumes `publishDiagnostics` from any running LSP
server in the workspace. Once the VSCode client above is running `dl --lsp`, the
`diag` rows are visible to a Claude Code session in that workspace with no second
integration: the "that is not allowed" rule is a datalog row, surfaced the same
way rust-analyzer surfaces a borrow error.

## Why this and not a bespoke linter

The lint is a datalog rule over the same fact base as every other query. A rule
can join `diag` against the call graph, the module graph, or `unused(...)` — so
"`.unwrap()` only flagged in code reachable from `main`" is a join, not new
plumbing. The linter is a query; richer lints are richer queries.
