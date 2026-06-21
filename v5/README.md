# sprefa v5 (`dl`)

Datalog over code, in repo/rev/time space. Extract facts from source files with
`scan` + located matchers, write recursive rules, and the engine lowers them to
a SQLite fixpoint. Sinks turn result rows into query output (`?`), editor
diagnostics (`diag` + `--lsp`/`--check`), or generated/spliced files (`gen`).
~14k LOC, sync-tick, no compiler dependency.

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
| module import | `use "std/callgraph.dl".` | splice another `.dl` file's items here; see [Modules](#modules) |
| template decl | `def name(p1, p2) <- body, body.` | parameterized rule body, inlined at call sites; see [Templates](#templates-def) |
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
| `scan` | `scan(glob, path, rev_out)` or `scan(rev, glob, path, rev_out)` or `scan(repo, rev, glob, path, rev_out)` | select files. 3-ary defaults `repo="."` self and `rev="WORK"` worktree; 4-ary defaults `repo="."`; 5-ary names a repo coordinate. `rev` ∈ `"WORK"` (worktree) \| `"HEAD"` \| any git rev. `repo` ∈ config slug \| `"."` (self) \| `"*"` (fan over every configured repo) |
| `match` | `match(path, rev, /re/, line[, id])` | regex over file content, one row per match line. `(?<cap>..)` named groups bind dl vars of the same name; `$cap` is sugar for a lazy named group (`/TODO\($who\)/`); bare `$` stays the anchor. Optional `id` binds the whole-match span's spine id (deterministic from span+source, equals `insert_spine_where_bytes`'s id), so `ref(id, _, _, lo, hi)` resolves to the exact match and feeds `gen(:mode, path, lo, hi, ...)`. When `id` is present the whole-match span is pushed; the 4-arg form pushes named captures only |
| `ast` | `ast(path, rev, :rust\|:c\|:kotlin, "(query) @cap", line[, end])` | tree-sitter query; `@cap` captures bind same-named vars |
| `sg` | `sg(path, rev, :lang, "$X.unwrap()", line[, col, end_line, end_col])` | ast-grep pattern; metavar `$X` binds dl var `X`. Lines 1-based, columns 0-based byte offsets. `:lang` ∈ rust, ts, tsx, js, py, go, json, c, cpp, kotlin (see [src/sg.rs](src/sg.rs)) |
| `json` | `json(path, rev, "a.*.b", out)` | dotted path over json/yaml/toml (dispatched by extension; `*` = any key/element). Value is located |
| `cmd` | `cmd(path, rev, "tool {file}", line, out)` | shell out per matched file, one row per stdout line. Cached by (file hash, rule text). Nonzero exit + stdout = findings; nonzero + empty = error |
| `comment` | `comment(path, rev, /open/[, /close/], l0, l1, label)` | comment-marker regions in ANY file type (marker detection by line prefix: `//`, `#`, `<!--`, `/*`, `--`, `*`). One regex = sequential dividers; two = paired BEGIN/END with LIFO nesting. `l0`/`l1` are 1-based marker lines; `label` is the open regex's first named group or the trimmed tail. The three outputs accept kwargs / `_`: bind only what you need (`comment(p, rev, /re/, label: name)`, defaulting the rest to `_`) or drop a slot with `_` (`comment(p, rev, /re/, l0, _, name)`). A typo'd name is a parse error. See [src/comment.rs](src/comment.rs) |

Source rules extract per file and cannot join derived relations — a check that
needs both is two rules: extract, then join (see [Rails](#git-hook--claude-code-hook)).

### Body constructs (derived rules)

| form | example |
|---|---|
| positive atom | `edge(f, t)` |
| negation / anti-join | `!round(t, _)` |
| comparison | `=` `!=` `<` `<=` `>` `>=` — `n >= 4`, `p != fs:src/db.rs` |
| regex constraint | `f =~ /^[A-Za-z]+$/` (SQLite REGEXP; the `/.../` is the unified regex literal — same form `match`/`comment`/`sg` use) |
| glob constraint | `p ~~ "src/*"` (SQLite GLOB) |
| closure | `closure(edge)` as the entire body — see below |
| int arithmetic | `+ - * / %` in rule heads (derived AND source) and comparison sides: `rank(p, line + 1) <- fns(p, line).`, `line * 2 > 4`. Usual precedence, parens OK. Never in a body atom (binding position). `/` after a value is division; elsewhere it opens a `/regex/` |
| string functions | `split(text, sep, idx)` and `replace(text, from, to)` in rule heads and comparison sides: `seg(path, split(path, "/", -1)) <- file(path).`, `kebab(w, replace(w, "_", "-")) <- name(w).`. `idx` is 0-based; negative counts from the end (`-1` = last segment). Out-of-range `split` drops the row (NULL filter). Unary minus parses (`-1` not `0 - 1`). A computed binding `ext = split(p, ".", -1)` binds `ext` for later use in the same body. `replace` is SQLite-native; `split` is the `sprf_split` UDF |

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

## Modules

`use "path".` splices another `.dl` file's items at the import site. The
smallest viable module system: file inclusion with canonical-path dedup, no
separate namespace, no exports. A program with no `use` is byte-for-byte
identical to one parsed by the older flat pipeline.

```
use "std/callgraph.dl".
? reaches("Engine", dst).
```

**Include roots** (first existing match wins, each is a container dir that
`use` paths resolve against):

1. The program file's directory (`use "lib.dl"` for a sibling;
   `use "std/foo.dl"` when the program lives next to a `std/` dir).
2. `$SPREFA_STD` (explicit override; the install / CI hand-lever).
3. The crate root (`v5/`), which ships a `std/` subdir. Lets examples and
   tests use `use "std/foo.dl".` without an install step.
4. The binary's parent directory (`<exe>/..`, the installed layout).

**Diamond imports load once.** The canonical-path cache keys every loaded
file, so a second `use` of an already-loaded module is a no-op (its items do
not splice twice).

**Rel dedup.** Two declarations of the same `rel` with the same cols collapse
to one. The same name with conflicting cols is a hard error naming both col
vectors (a typo silently shadowing a library rel is the import story's worst
failure mode). Rules and queries splice verbatim.

**Shipped stdlib** lives in [std/](std/):

| file | exposes |
|---|---|
| [std/callgraph.dl](std/callgraph.dl) | `def`, `use`, `calls`, `reaches`, `unused` — the file-scoped call graph |
| [std/parsers/openapi.dl](std/parsers/openapi.dl) | `spec_op(op)` — operationIds from any `openapi.{json,yaml,yml}` in scope |

## Templates (`def`)

`def name(p1, p2) <- body.` declares a parameterized rule body. A body atom
`name(args)` is **inlined**: the template body is cloned, params are
substituted by the args, and every non-param internal var is alpha-renamed so
two instantiations of the same template never capture each other.

```
rel edge(a: int, b: int).
rel four_hop(a: int, b: int).
edge(1, 2). edge(2, 3). edge(3, 4). edge(4, 5).

def via(x, z) <- edge(x, m), edge(m, z).

four_hop(a, b) <- via(a, mid), via(mid, b).
? four_hop(a, b).
```

The two `via` calls expand to disjoint internal vars
(`__via_0_m` and `__via_1_m`) so the chain is four edges, not collapsed to two.

**The arity-reuse layer.** The plan's motivating example is the qualified call
graph (`fndef` + `callsite` + range containment). Without `def`, every program
that wanted this join would copy-paste the three atoms. With `def`, a library
ships the shape and programs call it with their own inputs:

```
def qcall(caller, callee) <-
  fndef(caller, p, s, e),
  callsite(callee, p, l),
  s <= l, l <= e.

calls(caller, callee) <- qcall(caller, callee).
```

**Contract.** Inline-only. No recursion, no fixed-point. A template that
transitively calls itself is rejected at expand time. A `def` from an imported
file is callable from the importer (the template table is scoped to the whole
merge). A same-name second `def` is a conflict. A `def` with no call site
emits zero rules (the template's own `rel` is undefined).

**Forward references.** Templates may be declared after the rules that call
them: the inline pass runs once the whole program (including transitive
`use`s) is collected.

**`def` as a rel name.** A rule whose head is literally `def(...)` still
parses as a rule because the second token is `(`, not an ident. Mirrors the
`use`-as-rel-name guard so existing programs keep parsing.

## Built-in relations

Reserved names, populated lazily — a program pays only for what it references. The lazy indexers (`type_entity`, `call_def`, `call_kind`, `df_node`, `loop_over`, `nest`) populate only over files a `scan` rule in the program pulls in; referencing one without a scan yields zero rows. See `examples/lsp-def-target.dl` for the `index_over` bridge pattern.

| relation | columns | source |
|---|---|---|
| `repo` | `(id, slug, root)` | configured repos + self |
| `rev` | `(id, repo, oid, ts)` | git revs seen by scans |
| `content` | `(id, hash)` | content addresses |
| `file` | `(repo, rev, path, content)` | scanned files |
| `changed` | `(path)` | `git status --porcelain -uall` vs HEAD: modified, added, renamed, untracked. Empty outside git. The rails join |
| `changed_line` | `(path, line)` | new-side lines of `git diff -U0 HEAD` hunks (`@@ -a,b +c,d @@` -> c..c+d-1) plus every line of untracked files (omitted by the diff). Pure-deletion hunks (d=0) emit nothing. Line-scoped rails precision |
| `true` | `()` | zero-arity singleton; the always- succeeds atom |
| `module_import` | `(file, rev, specifier, kind, line)` | import statements, Rust + TS + Kotlin. Kotlin adds `kind="same-package"` rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files |
| `module_edge` | `(src, dst)` | resolved file-to-file import graph (rev-deduped union) |
| `module_edge_rev` | `(src, dst, rev)` | rev-aware form |
| `module_unresolved` | `(file, specifier, reason, line)` | broken imports (the linter question) |
| `module_unresolved_rev` | `(file, rev, specifier, reason, line)` | rev-aware form |
| `crate_edge` | `(src, dst, kind, rev)` | workspace-internal Cargo dependency edges |
| `type_edge` | `(from, to, kind)` | type graph, Rust (syn) + Kotlin (tree-sitter) + TS (oxc); `kind` ∈ `field`\|`variant`\|`impl`\|`generic`. Kotlin: an interface's supertypes are `generic` (mirrors Rust supertraits), a class/object's are `impl`, val/var constructor params + body properties are `field`, enum entries are `variant` |
| `type_edge_rev` | `(from, to, kind, rev)` | rev-aware form (WORK-vs-HEAD type diff) |
| `type_entity` | `(sym, name, kind, parent, file, line)` | every declared type; `sym` is `file::kind::name` (the join key across graphs). When a SCIP index is present, `scip_ref` overrides name resolution |
| `type_sig` | `(sym, slot, pos, ref)` | type signature slots (params, fields) per sym |
| `type_link` | `(src, dst, kind)` | cross-type links not carried by `type_edge` |
| `call_def` | `(sym, kind, file, line, end)` | every callable; `sym` is `file::kind::name` |
| `call_site` | `(caller, callee, file, line)` | each call occurrence; `caller` is the resolved fn sym, `callee` is the bare callable text. `changed_line(p, l)` joins here for line-scoped rails |
| `call_edge` | `(caller, callee, kind)` | resolved caller-sym -> callee-sym edge; callee resolved via single-def or SCIP override |
| `call_edge_rev` | `(caller, callee, kind, rev)` | rev-aware form |
| `call_name` | `(sym, name)` | def sym -> bare callable name; resolves a `call_site` callee to candidate def syms |
| `call_kind` | `(fn, kind)` | per-fn read/write classification of its call sites; `kind` is `read` or `write`, classified from the bare callee name (execute/execute_batch/execute_returning -> write; prepare/query_row/query_map/query_and_then/query_named -> read). Join on `call_kind(fn, "write")` to ask "does this fn write". Table is rusqlite-shaped on purpose — collection-shaped names (insert/update/delete) are dropped to avoid false positives |
| `df_node` | `(id, kind, var, fn, file, line)` | intra-procedural dataflow node (`call_res`/`assign`/...); `id` is `file::line::kind` |
| `df_edge` | `(from, to)` | dataflow dependency |
| `loop_over` | `(file, start, end, var, collection, fn)` | one row per loop with its span, iter var, and collection |
| `allocates` | `(fn)` | one row per fn whose body builds a collection (Vec/HashMap/String ctor, `.collect`/`.clone`/`.to_string`) |
| `nest` | `(call_id, loop_id, depth, collection)` | one row per (call, enclosing loop); `depth` is nesting rank (1=outermost). Raw material for symbolic Big-O over `call_edge` |
| `doc_node` | `(file, line, kind, name, parent)` | structural nodes from non-source text (markdown: `heading`/`code_block`, via `tree-sitter-md` block grammar; ATX + setext headings, fenced + indented code blocks). `parent` is the enclosing heading. Emitted by the `ingest::IngestLang` registry |
| `doc_ref` | `(file, line, sym, kind, matched_name)` | doc→code bridge: name-matches `doc_node` headings to `type_entity` symbols (exact + normalized: articles/kind-words stripped), and scans code-block text for identifier mentions. `kind` is the doc_node kind (`heading`/`code_block`); `matched_name` is the doc-side string that matched. Empty unless the program also uses type relations |
| `scip_def` | `(symbol, file)` | from an existing `index.scip` at root or `$SPREFA_SCIP_INDEX` |
| `scip_ref` | `(file, symbol, def_file)` | compiler-backed references |
| `scip_edge` | `(src, dst)` | file-to-file SCIP dependency edges |
| `string` | `(id, text, norm)` | interned strings (ref spine) |
| `ref` | `(id, string, file, lo, hi)` | byte span per interned string; `id` is the rewrite coordinate. "Where does Foo occur": `string(s, "Foo", _), ref(_, s, f, lo, hi)` |

Declarations live in [src/engine.rs](src/engine.rs) (the `BUILTIN_RELS`
through `SPINE_RELS` const families + their `*_rel_decls` functions).

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

`textDocument/hover` auto-synthesizes a markdown summary from `type_entity` +
`call_def` (no rel to declare; opt in by referencing those rels).
`textDocument/definition` consults a program-declared `def_target(name, file,
line, kind)` rel first — go-to-def lands on real definition lines, not just
import-specifier module edges. See [docs/lsp.md](docs/lsp.md).

The [vscode-dl extension](editors/vscode-dl/) also ships a TextMate grammar
(comments, keywords, the `<-` arrow, regex/string/scheme-literal coloring) so
`.dl` files render with syntax highlighting, not plain text.

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

[[repos]]
slug = "delta/four"
root = "/path/to/maybe-delta"
allow_missing = true                    # missing root is non-fatal: scan yields
                                        # zero rows, engine prints one stderr line.
                                        # The slug is omitted from `repo(...)` so a
                                        # program can derive `missing_repo(S)` via
                                        # antijoin against its referenced set.
```

`scan("alpha/one", "WORK", glob, p, rev)` targets one repo;
`scan("*", "WORK", ...)` fans the rule over every configured repo. Or point
`--root` at a parent directory and use root-relative globs
(`"sprefa/v5/src/**/*.rs"`), as [examples/anim-deck.dl](examples/anim-deck.dl) does.

**Progressive analysis.** A multi-repo program does not have to bail when one
clone is missing. Mark the config row `allow_missing = true` and the engine
prints one stderr line, omits the slug from the `repo` builtin, and proceeds.
A program surfaces the miss for an agent or UI to route:

```
rel referenced(slug: text).
rel candidate_url(slug: text, url: text).
rel missing_repo(slug: text, hinted_url: text).

referenced("dep-a").
candidate_url("dep-a", "https://github.com/org/dep-a").

missing_repo(s, u) <- referenced(s), !repo(s, _, _), candidate_url(s, u).
? missing_repo(s, u).
```

Worked example: [examples/missing-repo.dl](examples/missing-repo.dl).

## Examples

All in [examples/](examples/), runnable as `dl examples/<name>.dl --root .`:

| file | shows |
|---|---|
| [glean.dl](examples/glean.dl) | the 5 canonical questions: definitions, callers, blast radius, type fan-in, broken imports |
| [callgraph.dl](examples/callgraph.dl) + `callgraph-{ast,sg,c,typed,resolved}.dl` | call graphs at increasing precision |
| [typegraph.dl](examples/typegraph.dl) | `type_edge` + closure: type blast radius |
| [lint-unwrap.dl](examples/lint-unwrap.dl) | `sg` spans → tight LSP squiggles |
| [lint-imports.dl](examples/lint-imports.dl) | `module_unresolved` as a check |
| [lint-docs.dl](examples/lint-docs.dl) | doc hygiene as agent rails: `needs-doc` (new + >2 refs + no `doc_ref`) + `chat-comment` (`///` block over an arity-derived budget). Patterns: sum-aggregate over a union, `max` for contiguous-block detection |
| [rails.dl](examples/rails.dl) | diff-scoped agent rails: banned words, exemptions via `fs:` literals, aggregate budgets |
| [rails-call-kind.dl](examples/rails-call-kind.dl) | the `call_kind` write-precision cut: warn on `.conn()` only when the enclosing fn actually writes (execute/execute_batch), not just reads (prepare/query_row) |
| [ban.dl](examples/ban.dl) | minimal banned-pattern check |
| [string-fns.dl](examples/string-fns.dl) | `split` / `replace` / computed bindings / unary minus / NULL-drop over v5's own fns |
| [openapi.dl](examples/openapi.dl) | `json` op + anti-join over a spec |
| [openapi-lsp.dl](examples/openapi-lsp.dl) | OpenAPI ↔ code cross-link as `diag` rows (the spine joins across TS/RS by shared operationId string) |
| [lsp-def-target.dl](examples/lsp-def-target.dl) | `def_target` declaration that drives go-to-def to real definition lines, with the `=~` regex literal routing type vs fn kinds |
| [time.dl](examples/time.dl) | cross-rev diff (WORK vs HEAD) |
| [module-history.dl](examples/module-history.dl) | rev-aware module graph |
| [repo-nearest.dl](examples/repo-nearest.dl) | multi-repo queries |
| [gen-type-table.dl](examples/gen-type-table.dl) | the marker-splice codegen loop: `comment` + `gen` keep a table fresh inside the program's own comments |
| [gen-doc-index.dl](examples/gen-doc-index.dl) | dogfoods the doc tools together: `doc_node` (markdown titles) + `comment` (Rust `//!` module docs) → one `gen` splice. Query-time doc/code unification |
| [auto-doc.dl](examples/auto-doc.dl) | the gen FILE-sink form: render `type_entity` rows to a fresh markdown reference (no marker pair, regenerated each tick, converges). The lexical indexer end-to-end as a doc generator |
| [anim-deck.dl](examples/anim-deck.dl) | cross-repo splice: aggregates + round tiers written into a slide deck's d2 fences |
| [typegraph-anim.dl](examples/typegraph-anim.dl) | gen → d2 `steps:` boards, `d2 --animate-interval` |
| [typeports.dl](examples/typeports.dl) | hub structs as d2 `sql_table` nodes, wires anchored to field rows |
| [missing-repo.dl](examples/missing-repo.dl) | `allow_missing` config + antijoin-derived `missing_repo(slug, url)` for the clone prompt |

## Where things live

| path | contents |
|---|---|
| [src/parse.rs](src/parse.rs) / [src/lex.rs](src/lex.rs) / [src/ast.rs](src/ast.rs) | DSL grammar; `ast.rs` is the syntax's single source of truth |
| [src/frontend.rs](src/frontend.rs) | module surface: `use` inclusion + `def` inlining (`load_program`, `expand_with`, `inline_template_calls`) |
| [src/engine.rs](src/engine.rs) | tick loop, fixpoint lowering, built-in relation refresh, gen writes |
| [src/typecheck.rs](src/typecheck.rs) | brands, anchors, path-literal resolution, stratification diags |
| [src/lower.rs](src/lower.rs) | rule → SQL |
| [src/db.rs](src/db.rs) | the plural-only SQL chokepoint (`insert_rows`); per-row writes are counted and screamed about |
| [src/comment.rs](src/comment.rs) | comment-marker region scanner |
| [src/modgraph.rs](src/modgraph.rs) | Rust+TS import resolver |
| [src/typegraph.rs](src/typegraph.rs) | type graph: Rust (syn) + Kotlin (tree-sitter) + TS (oxc) type-edge extractor; the `TypeLang` registry |
| [src/ingest/mod.rs](src/ingest/mod.rs) | document ingestion: the `IngestLang` registry + `doc_node` extractor (markdown via `tree-sitter-md` block grammar) |
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
- **String manipulation** — `${}` concat; `split(text, sep, idx)` and
  `replace(text, from, to)` in heads/comparisons (see Body constructs above);
  no substr, no regex capture over an already-bound value (use the `match`
  source op at scan time with `(?<name>...)` groups instead).
- **Closure in a mixed rule body is literal-seeded only** — dynamic transitive
  closure is a seeded point query, not a fixpoint join. Recursive rules over
  the `_edge` relations substitute (see [examples/callgraph.dl](examples/callgraph.dl)).
- **`doc_node` bridges to code via `doc_ref`** — markdown headings/code-blocks
  extract today, and `doc_ref(file, line, sym, kind, matched_name)` matches those
  headings to `type_entity` symbols (exact + normalized), plus scans code-block
  text for identifier mentions. Comments are not auto-extracted (syn strips
  them); the `comment` op pulls regions on demand at query time instead.
- **No per-language symbol literal** — `rs:`/`kt:`/`ts:`/`md:` addressing is
  deferred. Symbols are reachable today via column conjunctions (`name = "tick"`
  + `file = fs:src/engine.rs`); a terse module-path literal would collapse that
  but needs a resolver/UDF choice (see chat_log session 4).
