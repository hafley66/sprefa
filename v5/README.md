# sprefa v5 (`dl`)

Reactive datalog over code, in repo/rev/time space. Extract facts from source
with `scan` + located matchers (`regex`/`ast`/`sg`/`json`), write recursive
rules, and the engine lowers them to a SQLite fixpoint. ~6.4k LOC, SQLite-welded,
sync-tick.

It answers the canonical code-navigation questions — definition site, callers,
blast radius, type fan-in, broken imports — from `syn` + tree-sitter facts, no
compiler. See [examples/glean.dl](examples/glean.dl).

## Build & run

```
cargo build                       # builds the `dl` binary
cargo run --bin dl -- examples/glean.dl --root .
cargo test
```

`--root` defaults to the nearest `.git` ancestor of the program file.

## The model

- **Coordinates.** Every fact is keyed on `(repo, path, rev)`. Files are
  content-addressed (blake3 for the working tree, blob OID for a git rev), so
  the same path in two repos or two revs never collides. Contract pinned in
  [docs/data-model.md](docs/data-model.md).
- **Facts.** `scan(repo, glob, path, rev)` selects files; a matcher extracts
  rows from each.
- **Rules.** `head(..) <- body.` — ordinary datalog, recursive. Lowered to a SQL
  fixpoint loop.
- **Located spine.** Every matched value records its byte span in
  `ref(id, string, file, lo, hi)`, joinable to `string(id, text, norm)`. This is
  what makes a match a *coordinate* you can squiggle (LSP) or rewrite (`--move`).

## DSL surface

| Form | What it does |
|---|---|
| `scan("WORK"\|"HEAD"\|rev\|"*", glob, path, rev)` | select files; `"*"` fans over every configured repo |
| `match(path, rev, /re (?<cap>..)/, line)` | regex capture over file content |
| `ast(path, rev, :rust\|:c, "(query) @cap", line, end)` | tree-sitter query |
| `sg(path, rev, :lang, "pattern", line, ..)` | ast-grep pattern |
| `json(path, rev, "a.*.b", out)` | dotted path over json/yaml/toml (by extension; `*` = any value/element); value is **located** |
| `head(..) <- a(..), b(..).` | rule (recursive allowed) |
| `reaches(s,d) <- closure(edge).` | transitive closure (seed one endpoint with a literal to query) |
| `!rel(..)` | negation / anti-join |
| `x = y` `!=` `<` `<=` `>` `>=` | comparison |
| `x =~ "re"` · `x ~~ "glob"` | regex / glob constraint (SQLite `REGEXP`/`GLOB`) |
| `"${var}::${name}"` | string interpolation |
| `? rel(..).` | query (literal in a position pins/seeds it) |

Built-in relations include `module_import`/`module_edge`/`module_unresolved`
(import graph, cross-language Rust+TS), `type_edge` (syn type graph),
`crate_edge`, the `string`/`ref` spine, and `scip_*` when a SCIP index is present.

## CLI

| Flag | Effect |
|---|---|
| `dl prog.dl` | run; print `?` query results as a TSV block |
| `--root <dir>` | source root (default: nearest `.git`) |
| `--check` | render `diag` to stderr, exit non-zero on any `error` row (CI / pre-commit) |
| `--diag-json` | `--check` as a JSON array on stdout |
| `--query-json` | `?` results as JSON-lines (`{query, columns, rows, count}`) |
| `--lsp` | LSP server over stdio: the `diag` relation becomes live editor diagnostics |
| `--watch` | re-tick on file changes |
| `--changed <path>` | drive one incremental tick for changed paths (repeatable) |
| `--move OLD=NEW [--repo <slug>\|*] [--fix]` | rewrite `use`-path references for a module move (dry-run unless `--fix`) |
| `--db <path>` | persist to a SQLite file (default: in-memory) |

## Examples

`examples/` — [glean.dl](examples/glean.dl) (the 5 canonical questions),
`callgraph*.dl` (ast/sg/c/typed/resolved variants), [typegraph.dl](examples/typegraph.dl),
[lint-imports.dl](examples/lint-imports.dl) / [lint-unwrap.dl](examples/lint-unwrap.dl)
(diagnostics via `--check`/`--lsp`), [openapi.dl](examples/openapi.dl) (json + anti-join),
[time.dl](examples/time.dl) (cross-rev diff), [module-history.dl](examples/module-history.dl),
[repo-nearest.dl](examples/repo-nearest.dl).

## What it does NOT have yet

The relational/graph/diff core is complete; *computing on values* is thin:

- **No aggregation** — no `count`/`sum`/`min`/`max`/group. The biggest gap.
- **`Value` is `Text | Int`** — no float; `<` is lexical, not semver-aware.
- **String manip is a modicum** — `${}` concat, the comparisons above, and
  regex *capture over files*; no split/replace/substr, and no regex over a bound
  string value.
- **Closure in a rule body is literal-seeded only** — dynamic transitive closure
  is a seeded point query, not a fixpoint join.
- **No `sh`** — external tools aren't a first-class fact source yet.

## Layout & docs

`v5/` is the active engine. `v3/`, `v4/` are prior iterations kept for
design-recovery; the original coordinate model lives in `../sprefa-archive-20260428`.
Design docs in [docs/](docs/): the data model, LSP notes, and research on a
portable relation-store seam, the SQLite×graph-theory landscape, and ext-library
extracts (Cozo / DBSP / petgraph / datafrog / lsp-server).
