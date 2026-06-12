# sprefa v5 (`dl`)

Datalog over code, in repo/rev/time space. Extract facts from source files with
`scan` + located matchers, write recursive rules, and the engine lowers them to
a SQLite fixpoint. Sinks turn result rows into query output (`?`), editor
diagnostics (`diag` + `--lsp`/`--check`), or generated/spliced files (`gen`).
~9k LOC, sync-tick, no compiler dependency.

This file is the reference for humans and agents alike: full DSL syntax, type
system, built-in relations, CLI, hook/LSP installation, and pointers to every
other doc in the tree.

## Install & run

```sh
cargo build                                    # debug binary at target/debug/dl
cargo install --path v5 --bin dl               # put `dl` on PATH (run from repo root)

dl examples/glean.dl --root .                  # run a program, print ? queries
dl --check --root <repo>                       # discovery mode: runs <repo>/.dl/*.dl
dl examples/lint-unwrap.dl --root <repo> --lsp # live LSP diagnostics
```

`--root` defaults to the nearest `.git` ancestor of the program file (or of the
cwd in discovery mode). Multi-repo analysis is configured in
`~/.config/sprefa/config.toml` (see [Multi-repo](#multi-repo)).

## The model

- **Coordinates.** Every fact is keyed on `(repo, path, rev)`. File content is
  content-addressed (blake3 for the working tree, blob OID for a git rev), so
  the same path at two revs or in two repos never collides. Contract pinned in
  [docs/data-model.md](docs/data-model.md).
- **Facts.** `scan` selects files; a source op (`match`/`ast`/`sg`/`json`/
  `cmd`/`comment`) extracts rows from each. Source-op rows are cached by
  (file content hash, rule text) — a re-tick only re-runs what moved.
- **Rules.** `head(..) <- body.` — ordinary datalog, recursion allowed,
  lowered to a SQL fixpoint loop. A converged tick writes nothing.
- **Located spine.** Matched values record their byte spans, queryable as
  `string(id, text, norm)` + `ref(id, string, file, lo, hi)`. A match is a
  coordinate you can squiggle (LSP) or rewrite (`--move`).

## Program structure

A `.dl` program is a sequence of items, each terminated by `.`:

| item | syntax | purpose |
|---|---|---|
| relation decl | `rel name(col: type, ...).` | declare a derived relation and its column types |
| brand decl | `type Name <: parent.` | named subtype of a base type or another brand; storage stays text, unification is checked |
| anchor decl | `anchor name = fs:body.` | named filesystem anchor (v1: only the default scan-root anchor is referenced) |
| rule | `head(..) <- body, body, ... .` | derive rows; recursion allowed |
| aggregate rule | `fan_out(F, count(T)) <- edge(F, T).` | head-position aggregation; plain head terms are the GROUP BY |
| closure rule | `reaches(a, b) <- closure(edge).` | transitive closure of a 2-col edge relation |
| gen (file) | `gen("docs/{x}.md", "row {y}") <- body.` | render rows to a file, grouped by rendered path |
| gen (splice) | `gen(p, l0, l1, "row {y}") <- body.` | replace lines strictly between two marker lines (pair with `comment`) |
| query | `? rel(a, b, "literal").` | print results; a literal pins that column |
| comment | `# ...` | to end of line |

### Types

Declared column types (`rel` and brand parents):

| keyword | storage | meaning |
|---|---|---|
| `text` | TEXT | any string |
| `int` | INTEGER | 64-bit integer |
| `path` | TEXT | repo-relative path |
| `file` | TEXT | path known to be a file |
| `dir` | TEXT | path known to be a directory |
| `repo` | TEXT | repo coordinate (config slug / path / `"."` self) |
| `rev` | TEXT | git rev coordinate |
| any brand | base's | `type Hub <: text.` then `rel hub(h: Hub, ...)` |

Type errors surface as diagnostics under `--check` and in `--lsp`
(`brand-mismatch`, `path-escapes-root`, `unknown-anchor`, `unknown-scheme`,
`coerce-text-path`). See [src/typecheck.rs](src/typecheck.rs).

### Terms

| form | example | notes |
|---|---|---|
| variable | `Path`, `x` | any ident; scope is the rule |
| wildcard | `_` | matches anything, binds nothing |
| string | `"WORK"` | |
| int | `42` | |
| interpolation | `"${mod}::${name}"` | build strings from bound vars |
| template hole | `"fan-out {n}"` | in `gen` row/path templates only |
| typed path literal | `fs:src/db.rs`, fs:\`src/db.rs\`, `glob:src/**/*.rs` | resolved against the scan root at lower time; a typo'd `fs:` path is a check error, never a silently unmatched string. Backtick-fence bodies containing spaces/specials |

### Source ops (body position, extract facts from files)

| op | signature | what it does |
|---|---|---|
| `scan` | `scan(rev, glob, path, rev_out)` or `scan(repo, rev, glob, path, rev_out)` | select files. `rev` ∈ `"WORK"` (worktree) \| `"HEAD"` \| any git rev. `repo` ∈ config slug \| `"."` (self, the 4-ary default) \| `"*"` (fan over every configured repo) |
| `match` | `match(path, rev, /re/, line)` | regex over file content, one row per match line. `(?<cap>..)` named groups bind dl vars of the same name; `$cap` is sugar for a lazy named group (`/TODO\($who\)/`); bare `$` stays the anchor |
| `ast` | `ast(path, rev, :rust\|:c\|:kotlin, "(query) @cap", line[, end])` | tree-sitter query; `@cap` captures bind same-named vars |
| `sg` | `sg(path, rev, :lang, "$X.unwrap()", line[, col, end_line, end_col])` | ast-grep pattern; metavar `$X` binds dl var `X`. Lines 1-based, columns 0-based byte offsets. `:lang` ∈ rust, ts, tsx, js, py, go, json, c, cpp, kotlin (see [src/sg.rs](src/sg.rs)) |
| `json` | `json(path, rev, "a.*.b", out)` | dotted path over json/yaml/toml (dispatched by extension; `*` = any key/element). Value is located |
| `cmd` | `cmd(path, rev, "tool {file}", line, out)` | shell out per matched file, one row per stdout line. Cached by (file hash, rule text). Nonzero exit + stdout = findings; nonzero + empty = error |
| `comment` | `comment(path, rev, /open/[, /close/], l0, l1, label)` | comment-marker regions in ANY file type (marker detection by line prefix: `//`, `#`, `<!--`, `/*`, `--`, `*`). One regex = sequential dividers; two = paired BEGIN/END with LIFO nesting. `l0`/`l1` are 1-based marker lines; `label` is the open regex's first named group or the trimmed tail. See [src/comment.rs](src/comment.rs) |

Source rules extract per file and cannot join derived relations — a check that
needs both is two rules: extract, then join (see [Rails](#git-hook--claude-code-hook)).

### Body constructs (derived rules)

| form | example |
|---|---|
| positive atom | `edge(f, t)` |
| negation / anti-join | `!round(t, _)` |
| comparison | `=` `!=` `<` `<=` `>` `>=` — `n >= 4`, `p != fs:src/db.rs` |
| regex constraint | `f =~ "^[A-Za-z]+$"` (SQLite REGEXP) |
| glob constraint | `p ~~ "src/*"` (SQLite GLOB) |
| closure | `closure(edge)` as the entire body — see below |
| int arithmetic | `+ - * / %` in rule heads (derived AND source) and comparison sides: `rank(p, line + 1) <- fns(p, line).`, `line * 2 > 4`. Usual precedence, parens OK. Never in a body atom (binding position). `/` after a value is division; elsewhere it opens a `/regex/` |

**Aggregation** is head-position only: `count` `sum` `min` `max`.
Non-aggregate head terms are the grouping key. `count`/`sum` produce `int`;
`min`/`max` carry the argument's type. A `count(...)` in body position is a
parse error.

```
rel fan_out(f: text, n: int).
fan_out(f, count(t)) <- edge(f, t).

rel tgt_round(t: text, r: int).
tgt_round(t, min(r)) <- round(f, r), edge(f, t).
```

**Closure**: `reaches(a, b) <- closure(edge).` materializes the transitive
closure of a 2-col relation (SCC-condensed, see [src/scc.rs](src/scc.rs)).
To ask a point query, pin an endpoint: `? reaches("Engine", x).` Closure in a
mixed rule body is literal-seeded only — dynamic closure joins are a known gap.

### Sinks

**`?` query** — `? rel(a, b).` prints a TSV block (or JSON-lines with
`--query-json`). A literal in any position filters. There is no `where`
clause; filter by nesting a derived rule.

**`diag`** — declare a relation named `diag` and the engine maps its columns BY
NAME into editor diagnostics (`--lsp`) or check output (`--check`). Required:
`path`, `line`, `msg`. Optional: `col`, `end_line`, `end_col`,
`severity` (`error`|`warn`|`info`|`hint`, default `warn`). Full convention in
[docs/lsp.md](docs/lsp.md).

**`gen`** — codegen. File form renders body rows through a `{var}` path + row
template, grouped by rendered path. Splice form replaces the lines strictly
between two marker lines, pairing with `comment`'s `l0`/`l1` coordinates:

```
rel block(p: file, l0: int, l1: int, name: text).
block(p, l0, l1, name) <- scan("WORK", "docs/tables.md", p, rev),
  comment(p, rev, /BEGIN: $name$/, /END:/, l0, l1, name).

gen(p, l0, l1, "{f} has fan-out {n}") <- block(p, l0, l1, "fanout"), fan_out(f, n).
```

Rows render in deterministic order; the write is skipped when bytes already
match (convergence = a second tick writes nothing). `gen` never runs under
`--check` or `--lsp`. Splices across multiple rules into one file batch into a
single bottom-up write. Worked loop: [examples/gen-type-table.dl](examples/gen-type-table.dl),
cross-repo deck maintenance: [examples/anim-deck.dl](examples/anim-deck.dl).

## Built-in relations

Reserved names, populated lazily — a program pays only for what it references.

| relation | columns | source |
|---|---|---|
| `repo` | `(id, slug, root)` | configured repos + self |
| `rev` | `(id, repo, oid, ts)` | git revs seen by scans |
| `content` | `(id, hash)` | content addresses |
| `file` | `(repo, rev, path, content)` | scanned files |
| `changed` | `(path)` | `git status --porcelain -uall` vs HEAD: modified, added, renamed, untracked. Empty outside git. The rails join |
| `module_import` | `(file, rev, specifier, kind, line)` | import statements, Rust + TS + Kotlin. Kotlin adds `kind="same-package"` rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files |
| `module_edge` | `(src, dst)` | resolved file-to-file import graph (rev-deduped union) |
| `module_edge_rev` | `(src, dst, rev)` | rev-aware form |
| `module_unresolved` | `(file, specifier, reason, line)` | broken imports (the linter question) |
| `module_unresolved_rev` | `(file, rev, specifier, reason, line)` | rev-aware form |
| `crate_edge` | `(src, dst, kind, rev)` | workspace-internal Cargo dependency edges |
| `type_edge` | `(from, to, kind)` | type graph, Rust (syn) + Kotlin (tree-sitter); `kind` ∈ `field`\|`variant`\|`impl`\|`generic`. Kotlin: an interface's supertypes are `generic` (mirrors Rust supertraits), a class/object's are `impl`, val/var constructor params + body properties are `field`, enum entries are `variant` |
| `type_edge_rev` | `(from, to, kind, rev)` | rev-aware form (WORK-vs-HEAD type diff) |
| `scip_def` | `(symbol, file)` | from an existing `index.scip` at root or `$SPREFA_SCIP_INDEX` |
| `scip_ref` | `(file, symbol, def_file)` | compiler-backed references |
| `scip_edge` | `(src, dst)` | file-to-file SCIP dependency edges |
| `string` | `(id, text, norm)` | interned strings (ref spine) |
| `ref` | `(id, string, file, lo, hi)` | byte span per interned string; `id` is the rewrite coordinate. "Where does Foo occur": `string(s, "Foo", _), ref(_, s, f, lo, hi)` |

Declarations live at [src/engine.rs:25](src/engine.rs) (`BUILTIN_RELS` through
`SPINE_RELS`).

## CLI

| invocation | effect |
|---|---|
| `dl prog.dl` | run; print `?` queries as TSV |
| `dl` (no positional) | discovery: merge every `<root>/.dl/*.dl` (filename order, shared `rel` decls dedupe); auto-cache at `.dl/cache.db` (gitignored automatically) |
| `--root <dir>` | source root (default: nearest `.git` ancestor) |
| `--db <path>` | persist to SQLite (default: in-memory; discovery mode defaults to the cache). Derived tables land as plain-TEXT `rel_<name>` tables, queryable by anything that reads SQLite |
| `--check` | render `diag` to stderr. Exit 0 clean, 2 on any `error`-severity row, 1 broken program |
| `--diag-json` | `--check` with diagnostics as a JSON array on stdout |
| `--query-json` | `?` results as JSON-lines `{query, columns, rows, count}` |
| `--lsp` | LSP server over stdio; `diag` rows become live squiggles |
| `--watch` | re-tick on file changes |
| `--changed <path>` | one incremental tick for changed paths (repeatable) |
| `--move OLD=NEW [--repo slug\|*] [--fix]` | full file move; dry-run prints the plan unless `--fix`. `.rs`: `use`-path rewriting against discovered crate roots — bare uses, brace heads, AND brace-inner leaves (`use crate::{old::A, b}`); the moved file's own `super::` imports re-anchor; `--fix` renames on disk and re-homes the `mod` decl (creating the parent-module chain, promoting a private decl to `pub(crate)` when it leaves the crate root). `.kt`: import rewriting from the package delta under the file's source root; `--fix` renames and rewrites the moved file's `package` decl (wildcard imports and same-package bare uses are counted loudly, not rewritten). A moved file's child `mod x;` decls do not follow — counted loudly |
| `--profile` (or `DL_PROFILE=1`) | log slow SQL statements (threshold `DL_PROFILE_SQL_MS`, default 25ms), per-repo×rev scan times, tick phase breakdown, per-tick statement counts, slow `cmd` invocations (≥250ms) |
| `--cmd-budget N` (or `DL_CMD_BUDGET`) | cap `cmd` invocations per tick; over budget errors loudly (never silent truncation). Default unlimited |

## Git hook / Claude Code hook

Rails = `diag` rules joined against `changed(p)`, so pre-existing repo debt
never fires — only the current diff can trip a check. Full doc:
[docs/rails.md](docs/rails.md), starter rules: [examples/rails.dl](examples/rails.dl).

1. Put rules in `<repo>/.dl/*.dl`.
2. **Git pre-commit**: commit `.githooks/pre-commit` containing
   `exec dl --check`, then once per clone:
   ```sh
   git config core.hooksPath .githooks
   ```
   Non-zero exit blocks the commit (`git commit -n` bypasses). Same command is
   the CI step. Grain caveat: `changed` is worktree-vs-HEAD, not staged-only.
3. **Claude Code**: `.claude/settings.json` in the repo:
   ```json
   {
     "hooks": {
       "PostToolUse": [
         { "matcher": "Edit|Write|NotebookEdit",
           "hooks": [{ "type": "command", "command": "dl --check" }] }
       ]
     }
   }
   ```
   Exit 2 feeds stderr back to the agent (blocking-hook contract); exit 1
   (broken rails) surfaces to the user only. No flags needed — root and
   program discovery are cwd-independent.

A rail is two rules (source ops cannot join relations):

```
rel diag(path: text, line: int, severity: text, code: text, msg: text).

rel todo_hit(p: file, l: int).
todo_hit(p, l) <- scan("WORK", "src/**/*.rs", p, rev), match(p, rev, /TODO/, l).

diag(p, l, "error", "no-todo", "TODO in a changed file") <- todo_hit(p, l), changed(p).
```

## LSP

```sh
dl <rules.dl> --root <repo> --lsp
```

Any program with a `diag` relation becomes a live linter: save a file, the
engine ticks that path, rows become squiggles. Save-driven, disk-truth,
deterministic. Editor glue (VSCode generic-LSP client settings) and the full
column convention: [docs/lsp.md](docs/lsp.md). Claude Code's IDE bridge
consumes the published diagnostics with no extra integration. Tight squiggles:
bind `sg`'s span outputs (`line, col, end_line, end_col`) straight into the
matching `diag` columns — [examples/lint-unwrap.dl](examples/lint-unwrap.dl).

## Multi-repo

`~/.config/sprefa/config.toml` (or `$SPREFA_CONFIG` / `$XDG_CONFIG_HOME`):

```toml
[[repos]]
slug = "alpha/one"
root = "/path/to/checkout-a"

[[repos]]
slug = "gamma/three"
root = "/path/to/cache/gamma"
url  = "git@github.com:org/gamma.git"   # cloned on first scan if root is absent
```

`scan("alpha/one", "WORK", glob, p, rev)` targets one repo;
`scan("*", "WORK", ...)` fans the rule over every configured repo. Or point
`--root` at a parent directory and use root-relative globs
(`"sprefa/v5/src/**/*.rs"`), as [examples/anim-deck.dl](examples/anim-deck.dl) does.

## Examples

All in [examples/](examples/), runnable as `dl examples/<name>.dl --root .`:

| file | shows |
|---|---|
| [glean.dl](examples/glean.dl) | the 5 canonical questions: definitions, callers, blast radius, type fan-in, broken imports |
| [callgraph.dl](examples/callgraph.dl) + `callgraph-{ast,sg,c,typed,resolved}.dl` | call graphs at increasing precision |
| [typegraph.dl](examples/typegraph.dl) | `type_edge` + closure: type blast radius |
| [lint-unwrap.dl](examples/lint-unwrap.dl) | `sg` spans → tight LSP squiggles |
| [lint-imports.dl](examples/lint-imports.dl) | `module_unresolved` as a check |
| [rails.dl](examples/rails.dl) | diff-scoped agent rails: banned words, exemptions via `fs:` literals, aggregate budgets |
| [ban.dl](examples/ban.dl) | minimal banned-pattern check |
| [openapi.dl](examples/openapi.dl) | `json` op + anti-join over a spec |
| [time.dl](examples/time.dl) | cross-rev diff (WORK vs HEAD) |
| [module-history.dl](examples/module-history.dl) | rev-aware module graph |
| [repo-nearest.dl](examples/repo-nearest.dl) | multi-repo queries |
| [gen-type-table.dl](examples/gen-type-table.dl) | the marker-splice codegen loop: `comment` + `gen` keep a table fresh inside the program's own comments |
| [anim-deck.dl](examples/anim-deck.dl) | cross-repo splice: aggregates + round tiers written into a slide deck's d2 fences |
| [typegraph-anim.dl](examples/typegraph-anim.dl) | gen → d2 `steps:` boards, `d2 --animate-interval` |
| [typeports.dl](examples/typeports.dl) | hub structs as d2 `sql_table` nodes, wires anchored to field rows |

## Where things live

| path | contents |
|---|---|
| [src/parse.rs](src/parse.rs) / [src/lex.rs](src/lex.rs) / [src/ast.rs](src/ast.rs) | DSL grammar; `ast.rs` is the syntax's single source of truth |
| [src/engine.rs](src/engine.rs) | tick loop, fixpoint lowering, built-in relation refresh, gen writes |
| [src/typecheck.rs](src/typecheck.rs) | brands, anchors, path-literal resolution, stratification diags |
| [src/lower.rs](src/lower.rs) | rule → SQL |
| [src/db.rs](src/db.rs) | the plural-only SQL chokepoint (`insert_rows`); per-row writes are counted and screamed about |
| [src/comment.rs](src/comment.rs) | comment-marker region scanner |
| [src/modgraph.rs](src/modgraph.rs) | Rust+TS import resolver |
| [src/typegraph.rs](src/typegraph.rs) | syn type-edge extractor |
| [src/scc.rs](src/scc.rs) | closure / SCC condensation |
| [src/spine.rs](src/spine.rs) / [src/datapath.rs](src/datapath.rs) | ref-spine IDs, located spans |
| [src/lsp.rs](src/lsp.rs) | the LSP server |
| [src/rspath.rs](src/rspath.rs) / [src/ktpath.rs](src/ktpath.rs) / [src/refactor.rs](src/refactor.rs) | `--move` rewriting (Rust use-paths / Kotlin imports) |
| [src/scip_import.rs](src/scip_import.rs) | SCIP index ingestion |
| [docs/data-model.md](docs/data-model.md) | the (repo, path, rev) coordinate contract |
| [docs/lsp.md](docs/lsp.md) | diag convention + editor setup |
| [docs/rails.md](docs/rails.md) | hook setup + exit-code contract |
| [docs/](docs/) | research: portable relation-store seam, SQLite×graph landscape, ext-library extracts (Cozo/DBSP/petgraph/datafrog/lsp-server) |
| [book/](book/) | the datalog-engine book: facts→rules→fixpoint→incremental→storage |
| [tests/](tests/) | e2e + the SCIP differential oracles (`oracle_rust.rs` vs rust-analyzer, `oracle_kotlin.rs` vs scip-java; both skip when the tool is absent) |
| `plans/`, `../CLAUDE.md` | task ledger and design plans |

`v5/` is the active engine. `v3/`/`v4/` are prior iterations kept for
design-recovery; the original coordinate model lives in
`../../sprefa-archive-20260428`.

## Known gaps

- **`Value` is `Text | Int`** — no float; `<` on text is lexical. Int
  arithmetic (`+ - * / %`) works in heads and comparisons.
- **String manipulation is thin** — `${}` concat and the constraints above;
  no split/replace/substr, no regex over an already-bound value.
- **Closure in a mixed rule body is literal-seeded only** — dynamic transitive
  closure is a seeded point query, not a fixpoint join.
