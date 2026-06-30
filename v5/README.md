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
- **Facts.** `scan` selects files; a source op (`match`/`ast`/`sg`/`json`/`jsonp`/
  `cmd`/`comment`) extracts rows from each. Source-op rows are cached by
  (file content hash, rule text) — a re-tick only re-runs what moved.
- **Rules.** `head(..) <- body.` — ordinary datalog, recursion allowed,
  lowered to a SQL fixpoint loop. A converged tick writes nothing.
- **Located spine.** Matched values record their byte spans, queryable as
  `string(id, text, norm)` + `ref(id, string, file, lo, hi)`. A match is a
  coordinate you can squiggle (LSP) or rewrite (`--move`).

## How it runs (the tick)

One tick is: refresh source facts → evaluate the fixpoint → fire sinks. The
same Tarjan SCC pass shows up on three different graphs, which is most of the
engine in one sentence.

1. **Source vs derived — the basis for everything.** `scan` + a source op
   extract SOURCE rows, each tagged with its file; rules derive the rest. A
   source fact has exactly one support (its file), so an edit retracts exactly
   the rows tagged that file and re-extracts them — no reference counting. A
   derived fact has no home file, so it recomputes. This split is what makes
   incrementality tractable (deleting derived facts under recursion is the hard
   case; the design avoids it). See [book/04-incremental-maintenance.md](book/04-incremental-maintenance.md).
2. **The fixpoint** (`rebuild_derived`, [src/engine.rs](src/engine.rs)). One
   loop per stratum: apply every rule (`INSERT OR IGNORE ... SELECT`), repeat
   until a pass adds nothing. Monotone growth in a finite universe settles at
   the unique least fixpoint (Knaster–Tarski). Recursion is just a rule that
   names itself in head and body; a converged tick writes zero rows.
3. **Stratification** (`stratify`, engine.rs). `!rel` needs the negated relation
   finished first, so the engine SCCs the *rule* dependency graph (Tarjan
   again): a negative edge inside a cycle is rejected as not-stratifiable;
   otherwise relations layer and evaluate bottom-up. (Temporal `@next` carries
   read the *prior* tick, so they legitimately break a cycle the static checker
   still reports — `--check` over-flags those programs; the tick runs them.)
4. **Incremental re-tick.** A source rel whose `(content-hash, rule-text)`
   digest is unchanged is pruned before evaluation; only moved files re-extract.
   The derived layer is keyed on a digest of the whole derived program and skips
   when nothing it reads changed (the recompute-guard rail enforces this for
   from-scratch ops like graph embedding).
5. **Closures as a condensed walk** (`src/scc.rs`). The full `reaches` relation
   is Θ(V²) on a cyclic graph. Tarjan collapses each cycle to a super-node in
   O(V+E); the remainder is a DAG, and a point query ("what does X reach?") is a
   seeded BFS over the condensed edges (reverse edges answer "who reaches X?").
6. **Auto-index** (`auto_indexes`, engine.rs). Every column a variable shares
   across ≥2 body atoms is indexed, so a join seeks instead of scanning.

Full derivations, citations, and exercises live in [book/](book/) (ch. 2
fixpoint, 3 cycles, 4 incremental, 7 the fast paths).

## Speed

`dl` lowers to SQLite, so performance is "pick the right loop, then let the
B-trees do the join." The shape that matters is which fast path a query hits,
not raw constant factors. Measured on a Linux-kernel checkout (the stress
fixture):

| workload | naive | fast path | what changed |
|---|---|---|---|
| call-graph join (`fndef` F≈16k × `callsite` C≈96k) | 22s (~1.5e9 row touches) | 1.9s | auto-index on the shared join key (`path`) |
| point query (what does X reach?) | ~2s (SQL recursive view) | 30µs | seeded BFS over condensed edges, not a full closure then filter |
| condense the call graph (23k edges) | — | milliseconds | Tarjan SCC, one DFS, O(V+E) |

The honest baseline this grew from: a correct-but-unindexed run of kernel
reachability was 197s and the resolved call graph 30s before these paths went
in. Two refinements are deliberately *not* taken yet: semi-naive is half-done
(the loop re-joins the full relation each round and lets `INSERT OR IGNORE`
discard duplicates, rather than joining only the new frontier), and indexes are
single-column equality only (a range join such as `s <= l <= e` still scans).
Both are documented in [book/07-the-fast-paths.md](book/07-the-fast-paths.md).

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
| gen (file) | `gen("docs/{x}.md", "row {y}") <- body.` | render rows to a file, grouped by rendered path; one rule per file |
| gen (append) | `gen(:append, "docs/x.md", "row {y}") <- body.` | render to a file where MANY rules concatenate in program order; assemble a header rule + a rows rule into one page, no markers (see `examples/gen-reference.dl`) |
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

<!-- BEGIN: op-table -->
| op | signature | what it does |
|---|---|---|
| `ast_yaml` | `ast_yaml(path, rev, :lang, "rule yaml", line, ...)` | ast-grep `RuleCore` YAML body (usually backtick/multiline); mirrors `sg()` but the 4th arg is a relational rule (`inside:`/`has:`) instead of a pattern string. Span outputs share the `sg` kwarg/`_` form. See [src/sg.rs](src/sg.rs) |
| `ast` | `ast(path, rev, :rust\|:c\|:kotlin, "(query) @cap", line[, end])` | tree-sitter query; `@cap` captures bind same-named vars |
| `cmd` | `cmd(path, rev, "tool {file}", line, out)` | shell out per matched file, one row per stdout line. Cached by (file hash, rule text). Nonzero exit + stdout = findings; nonzero + empty = error |
| `comment` | `comment(path, rev, /open/[, /close/], l0, l1, label)` | comment-marker regions in ANY file type (marker detection by line prefix: `//`, `#`, `<!--`, `/*`, `--`, `*`). One regex = sequential dividers; two = paired BEGIN/END with LIFO nesting. `l0`/`l1` are 1-based marker lines; `label` is the open regex's first named group or the trimmed tail. The three outputs accept kwargs / `_`: bind only what you need (`comment(p, rev, /re/, label: name)`, defaulting the rest to `_`) or drop a slot with `_` (`comment(p, rev, /re/, l0, _, name)`). A typo'd name is a parse error. See [src/comment.rs](src/comment.rs) |
| `json` | `json(path, rev, q:{ $k: $v })` | declarative brace pattern over json/yaml/toml (dispatched by extension). Each match binds N named captures (keys AND values) as dl vars, like match's named groups. The `q:{...}` arg is a structured `q:` literal (highlightable, not a string). `{ name: $n }` descends by exact key; `{ $k: $v }` iterates entries; `{ a: $a, b: $b }` is conjunctive; `{ **: { image: $i } }` recurses at any depth; `[...$x]` spreads arrays; `re:REGEX` / glob (`*id`) keys |
| `jsonp` | `jsonp(path, rev, "a.*.b", out)` | dotted path over json/yaml/toml (dispatched by extension; `*` = any key/element). Value is located. The dotted-string form; the declarative brace pattern is `json` |
| `match` | `match(path, rev, /re/, line[, id][, col, end_col])` | regex over file content, one row per match line. `(?<cap>..)` named groups bind dl vars of the same name; `$cap` is sugar for a lazy named group (`/TODO\($who\)/`); bare `$` stays the anchor. Optional trailing args after `line`, by count: 1 ⇒ `id` (the whole-match span's spine id, deterministic from span+source, equals `insert_spine_where_bytes`'s id), so `ref(id, _, _, lo, hi)` resolves to the exact match and feeds `gen(:mode, path, lo, hi, ...)`; 2 ⇒ `col, end_col` (the whole-match span's 0-based byte columns within `line`, for sub-line `diag` spans); 3 ⇒ `id, col, end_col`. When `id` is present the whole-match span is pushed; the 4-arg form pushes named captures only |
| `scan` | `scan(glob, path, rev_out)` or `scan(rev, glob, path, rev_out)` or `scan(repo, rev, glob, path, rev_out)` | select files. 3-ary defaults `repo="."` self and `rev="WORK"` worktree; 4-ary defaults `repo="."`; 5-ary names a repo coordinate. `rev` ∈ `"WORK"` (worktree) \| `"HEAD"` \| any git rev. `repo` ∈ config slug \| `"."` (self) \| `"*"` (fan over every configured repo) |
| `sg` | `sg(path, rev, :lang, "$X.unwrap()", line[, col, end_line, end_col][, id])` | ast-grep pattern; metavar `$X` binds dl var `X` (its matched text). Lines 1-based, columns 0-based byte offsets. Optional trailing `id` binds the WHOLE-match span's spine id (literal text included, not just the captures' bbox), so `ref(id, _, _, lo, hi)` + `gen(:replace, p, lo, hi, "{x}…")` is a metavar-templated structural rewrite (full ast-grep codemod). `:lang` ∈ rust, ts, tsx, js, py, go, json, c, cpp, kotlin (see [src/sg.rs](src/sg.rs)) |
<!-- END: op-table -->

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

<!-- BEGIN: builtin-rels -->
| relation | group | columns | summary |
|---|---|---|---|
| `agent_edit` | agent | `(harness, session, idx, path)` | every file edit in the latest agent turn, tagged harness+session+turn idx (from the at-rest harness store) |
| `agent_touch` | agent | `(harness, session, path)` | the latest agent turn's edited files (harness, session, path) |
| `allocates` | dataflow | `(fn)` | one row per fn whose body builds a collection (Vec/HashMap/String ctor, .collect/.clone/.to_string) |
| `call_def` | call | `(repo, sym, kind, file, line, end)` | every callable; sym is file::kind::name |
| `call_edge` | call | `(caller, callee, kind)` | resolved caller-sym to callee-sym edge (single-def or SCIP override) |
| `call_edge_rev` | call | `(caller, callee, kind, rev)` | rev-aware call_edge |
| `call_kind` | call | `(fn, kind)` | per-fn read/write classification from the bare callee name (execute* -> write, query*/prepare -> read); rusqlite-shaped, collection names dropped to avoid false positives |
| `call_name` | call | `(sym, name)` | def sym to bare callable name; resolves a call_site callee to candidate def syms |
| `call_site` | call | `(repo, caller, callee, file, line)` | each call occurrence; caller is the resolved fn sym, callee the bare text; changed_line joins here for line-scoped rails |
| `changed` | changed | `(path)` | git status --porcelain -uall vs HEAD (modified/added/renamed/untracked); empty outside git; the rails join |
| `changed_line` | changed | `(path, line)` | new-side lines of git diff -U0 HEAD hunks plus every line of untracked files; pure-deletion hunks emit nothing; line-scoped rails precision |
| `child` | node | `(parent, child)` | CST parent-child edges (exactly 2 cols, so closure(child) gives ancestry) |
| `clock` | clock | `(secs, bucket)` | the current time bucket now/secs per named period, present EVERY tick (not edge-triggered like every); clock(300,b) binds b to a monotone int advancing once per 300s — join it to vary a digest or gate on cadence, no @next counter |
| `content` | core | `(id, hash)` | content addresses |
| `crate_edge` | module | `(src, dst, kind, rev)` | workspace-internal Cargo dependency edges |
| `created` | created | `(path, name, email, ts)` | files added since their first appearance, with author name/email/timestamp |
| `df_edge` | dataflow | `(from, to)` | intra-procedural dataflow dependency edge |
| `df_node` | dataflow | `(id, kind, var, fn, file, line)` | intra-procedural dataflow node (call_res/assign/...); id is file::line::kind |
| `df_param` | dataflow | `(id, pos)` | (param df_node id, positional index); index counts typed params only (self skipped) so it aligns with type_sig.pos for node-level type joins |
| `doc_comment` | type | `(repo, sym, line, text)` | doc comment per type_entity sym: (repo, sym, line, text); AST-located per language (Rust #[doc] attrs, Kotlin KDoc sibling, TS leading /** */) |
| `doc_node` | doc | `(repo, file, line, kind, name, parent)` | structural nodes from non-source text (markdown headings + code blocks via tree-sitter-md: ATX/setext headings, fenced/indented blocks); parent is the enclosing heading |
| `doc_ref` | doc | `(repo, file, line, sym, kind, matched_name)` | doc-to-code bridge: name-matches doc_node headings to type_entity symbols (exact + normalized) and scans code blocks for identifier mentions; empty unless the program also uses type relations |
| `doc_tag` | type | `(repo, sym, tag, arg, text)` | structured doc tags per sym: (repo, sym, tag, arg, text); @param/@returns/@deprecated for JSDoc/KDoc, # Section headings for rustdoc |
| `effect_log` | effect | `(id, kind, head, state, args, req_tx)` | the @async/@stream drain queue: one row per request (id, kind, head rel, state queued/running/done/failed, args JSON, req_tx); the dl-native call log, queryable live and parity-comparable to an external cache's call log |
| `every` | clock | `(secs)` | holds interval N only on ticks that cross an N-second boundary (and the first tick); an every(30) body atom self-throttles its rule |
| `file` | core | `(repo, rev, path, content)` | scanned files, keyed by (repo, rev, path, content) |
| `head` | daemon | `(repo, name, oid)` | git HEAD per repo (repo, ref name, oid) |
| `loop_over` | dataflow | `(file, start, end, var, collection, fn)` | one row per loop with its span, iter var, and collection |
| `module_edge` | module | `(src, dst)` | resolved file-to-file import graph (rev-deduped union) |
| `module_edge_rev` | module | `(src, dst, rev)` | rev-aware module_edge |
| `module_import` | module | `(file, rev, specifier, kind, line)` | import statements (Rust + TS + Kotlin); Kotlin adds kind=same-package rows for bare uses of another file's column-0 decl, and an expect/actual decl fans edges to all declaring files |
| `module_unresolved` | module | `(file, specifier, reason, line)` | broken imports: a reference that resolved to no project file (the linter question) |
| `module_unresolved_rev` | module | `(file, rev, specifier, reason, line)` | rev-aware module_unresolved |
| `nest` | dataflow | `(call_id, loop_id, depth, collection)` | one row per (call, enclosing loop); depth is nesting rank (1=outermost); raw material for symbolic Big-O over call_edge |
| `node` | node | `(id, kind, file, lo, hi, parent)` | CST nodes (nested-set spans): id, kind, file, lo, hi, parent |
| `program` | daemon | `(path, hash, mtime)` | dl programs the daemon tracks (path, content hash, mtime) |
| `propose_clone` | propose | `(kernel, path, lo, hi, param)` | proposed clone/near-duplicate groups keyed by a shared kernel |
| `propose_extract` | propose | `(path, lo, hi, param)` | proposed extract-function refactor spans (path, lo, hi, param) |
| `ref` | spine | `(id, string, file, lo, hi)` | byte span per interned string; id is the rewrite coordinate — 'where does Foo occur' is string(s, Foo, _), ref(_, s, f, lo, hi) |
| `rel_catalog` | meta | `(name, group, cols, doc)` | this table: every built-in relation with its group, columns, and one-line doc |
| `repo` | core | `(slug, root, url)` | configured + dynamically-pulled repos whose root exists; writable as a sink — a repo(...) rule clones+registers when the github org is in `org` (hard filter); see docs/dynamic-reaching.md |
| `rev` | core | `(id, repo, oid, ts)` | git revs seen by scans |
| `rev_advanced` | daemon | `(repo, name, old, new)` | daemon signal that a repo ref advanced (repo, name, old oid, new oid) |
| `scip_callee_type` | scip | `(sym, type)` | receiver type parsed from a method moniker's impl/for segment |
| `scip_def` | scip | `(symbol, file)` | symbol defs from an existing index.scip (root or $SPREFA_SCIP_INDEX) |
| `scip_edge` | scip | `(src, dst)` | file-to-file SCIP dependency edges |
| `scip_fn_edge` | scip | `(caller, callee)` | function-level call edge; caller is the innermost enclosing fn def |
| `scip_impl` | scip | `(impl, iface)` | interface/supertype dispatch edge from SCIP is_implementation (impl to iface) |
| `scip_local` | scip | `(fn, name)` | local-variable + parameter declarations attributed to their enclosing fn |
| `scip_name` | scip | `(symbol, name)` | descriptor name (last identifier run) of a moniker, computed in-engine |
| `scip_ref` | scip | `(file, symbol, def_file)` | compiler-backed references (ref file, symbol, def file) |
| `similar` | embed | `(a, b, score)` | content-addressed nearest-neighbor pairs from the embedding backend, with score |
| `string` | spine | `(id, text, norm)` | interned strings (ref spine): id, text, normalized text |
| `true` | core | `()` | zero-arity singleton; the always-succeeds atom |
| `type_edge` | type | `(from, to, kind)` | type-graph edges across Rust (syn), Kotlin (tree-sitter), TS (oxc); kind is field/variant/impl/generic — Kotlin interface supertypes are generic, class/object impl, val/var ctor params + body properties field, enum entries variant |
| `type_edge_rev` | type | `(from, to, kind, rev)` | rev-aware type_edge (WORK-vs-HEAD type diff) |
| `type_entity` | type | `(repo, sym, name, kind, parent, file, line)` | every declared type; sym is file::kind::name, the cross-graph join key; scip_ref overrides name resolution when a SCIP index is present |
| `type_lgg` | type-shape | `(a, b, vars)` | least-general generalization of two type shapes (shape-iso experiment) |
| `type_link` | type | `(src, dst, kind)` | cross-type links not carried by type_edge (SCIP-resolved sym to sym) |
| `type_shape` | type-shape | `(name, hash)` | structural type-shape fingerprint per type (shape-iso experiment) |
| `type_sig` | type | `(sym, slot, pos, ref)` | type signature slots (params, fields) per sym |
<!-- END: builtin-rels -->

The table above is generated from the engine's self-describing `rel_catalog` by `examples/builtin-rels.dl` (run it, or `dl --load` it; the daemon regenerates on `engine.rs` edits). It is the single source of relation docs: group, columns, and the one-line summary all come from `builtin_rel_docs` + the `*_rel_decls` functions in [src/engine.rs](src/engine.rs), so the table can't drift from the declarations, and a new built-in is forced to appear by the doc-completeness test (`tests/it/rel_catalog.rs`). To document a new relation, add its `(name, group, summary)` row to `builtin_rel_docs` — do not hand-edit the block above; it is regenerated.

## CLI

Two usage forms, then the flag reference:

| invocation | effect |
|---|---|
| `dl prog.dl` | run; print `?` queries as TSV |
| `dl` (no positional) | discovery: merge every `<root>/.dl/*.dl` (filename order, shared `rel` decls dedupe); auto-cache at `.dl/cache.db` (gitignored automatically) |

The flag table below is generated from the clap `Cli` struct (each flag's
`///` doc-comment) by [examples/cli-doc.dl](examples/cli-doc.dl) via `dl
--cli-md`, so it can't drift from the parser: every flag auto-appears, and a
flag with no doc-comment renders an empty cell (visible drift). Do not
hand-edit between the markers; to change a row, edit the doc-comment in
[src/main.rs](src/main.rs), rebuild, and rerun the generator. Daemon
lifecycle, the two-daemon model (per-root vs rootless serving), the RPC
surface, and env vars: [docs/daemon.md](docs/daemon.md).

<!-- BEGIN: cli -->
| flag | effect |
|---|---|
| `--changed <CHANGED>` | Drive one incremental tick for these changed paths (the delta path the watcher uses), instead of a full run. Repeatable |
| `--check` | Lint/ban mode: render the `diag` relation to stderr. Exit 0 clean, 2 if any `error`-severity row exists (Claude Code's blocking-hook code), 1 on a broken program. For pre-commit / CI / Claude Code hooks. See docs/rails.md |
| `--cmd-budget <CMD_BUDGET>` | Cap `cmd` invocations per tick (or DL_CMD_BUDGET); over budget is a loud error, never a silent truncation. Default: unlimited |
| `--daemon` | Run as the long-lived daemon foreground (logs to stderr, ignores idle timeout). Usually invoked internally by spawn-if-missing; passing this flag explicitly is the debug path. See plans/2026-06-21-daemon-and-menu-bar.md |
| `--db <DB>` | Persist derived tables to a SQLite db at this path (default: in-memory; discovery mode defaults to `<root>/.dl/cache.db`). Derived relations land as plain-TEXT `rel_<name>` tables, queryable by anything that reads SQLite |
| `--diag-json` | Like --check but emit the diagnostics as a JSON array on stdout |
| `--fix` | With --move, write the rewritten files instead of previewing |
| `--load <LOAD>` | Load a script into the running daemon as a WATCHED program: joins the loaded set, runs on every tick, hot-reloads on edit. Omit `--root` to target the global rootless serving daemon |
| `--load-once <LOAD_ONCE>` | Load a script ONE-TIME: eval it on a throwaway engine, print the `?` query results, persist nothing. Same target rules as `--load` |
| `--lsp` | Run as an LSP server over stdio: the program's `diag` relation becomes live editor diagnostics (lint on open/save). See docs/lsp.md |
| `--move <MOVE>` | Auto-refactor: rewrite `use`-path references for a module move `OLD_FILE=NEW_FILE` (repo-relative Rust paths). Dry-run unless --fix. Repeatable. Ignores the `program` positional |
| `--no-daemon` | Force the in-process path this invocation (do not auto-attach). Same as `DL_NO_DAEMON=1`. Useful when the daemon socket is wedged |
| `--profile` | Profile mode (or DL_PROFILE=1): log slow SQL statements (threshold DL_PROFILE_SQL_MS, default 25), per-repo scan times, tick phase breakdown, and per-tick statement counts |
| `--query-json` | Emit `?` query results as JSON-lines (one object per query: {query, columns, rows, count}) instead of the human TSV block |
| `--repo <REPO>` | With --move, which repo to rewrite: a config slug, or `*`/`all` for every configured repo. Omitted = the --root repo (self) |
| `--root <ROOT>` | Source root. When omitted, defaults to the nearest `.git` ancestor of the program file (the repo it lives in), else the current directory |
| `--stdio` | Ignored no-op alias for `--lsp`. vscode-languageclient, coc.nvim, and neovim's lspconfig all append `--stdio` when spawning an LSP server; accept it so `dl` drops into any client without extension-specific arg gymnastics. Stdio is the only transport either way |
| `--stop` | Send `shutdown` to the daemon on `<root>/.dl/daemon.sock` and exit |
| `--tick-audit` | After each tick, print every relation's row count (or DL_TICK_AUDIT=1) |
| `--tray` | With --daemon: spawn the menu bar tray icon (macOS v1; Windows/Linux deferred). The main thread runs the tray event loop; the accept loop moves off-main. Implies --daemon |
| `--verify <VERIFY>` | Verify-rollback: run the program (applying `gen` edits), then run this shell command as a checker in the root. Keep the edits only if it exits 0; otherwise restore every touched file to its pre-run state and exit 1. Transactional codemod — apply, test, keep-if-pass. See christmas #14 |
| `--watch` | Re-tick on file changes in the source root (in-process watcher, the pre-daemon path). For the warm long-lived watcher, use `--daemon` |
<!-- END: cli -->

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
| [openapi.dl](examples/openapi.dl) | `jsonp` op + anti-join over a spec |
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

The highlights above are hand-picked. The **full corpus index** below is
generated by [examples/gen-reference.dl](examples/gen-reference.dl) (a `scan` +
`match` over every `examples/*.dl` first comment), as are the language reference
pages: [relations.md](docs/reference/relations.md),
[functions.md](docs/reference/functions.md),
[syntax.md](docs/reference/syntax.md) (every op's syntax + semantics from
`op_catalog`), and [examples.md](docs/reference/examples.md). Do not hand-edit
between the markers.

<!-- BEGIN: examples-index -->
| example | summary |
|---|---|
| [`agent-live.dl`](examples/agent-live.dl) | Live probe for the built-in agent-harness relations (agent.rs). |
| [`anim-deck.dl`](examples/anim-deck.dl) | Maintains the machine-written regions of the sprefa chapter in the anim deck |
| [`anim-self.dl`](examples/anim-self.dl) | The deck tours its own diff. Scan anim's TypeScript (readable by dl as of |
| [`author-test-clones.dl`](examples/author-test-clones.dl) | "What is the most-used / most-similar code in the test files created by ONE |
| [`auto-doc.dl`](examples/auto-doc.dl) | auto-doc.dl — generate a code reference doc from the lexical indexer. |
| [`autodoc-plans.dl`](examples/autodoc-plans.dl) | Autodoc: transclude tagged markdown comments from session logs into PLANS.md. |
| [`ban.dl`](examples/ban.dl) | Ban a code move with ast-grep + the CLI. Each rule is one banned pattern. |
| [`banned-word-guard.dl`](examples/banned-word-guard.dl) | Flag a banned word in the agent's OWN output — turn text OR a plan-tool body — |
| [`builtin-rels.dl`](examples/builtin-rels.dl) | builtin-rels.dl — render the engine's self-describing `rel_catalog` into the |
| [`call-seams.dl`](examples/call-seams.dl) | Call-seam finder: where can a big file be cut with the LEAST call traffic |
| [`callgraph-ast.dl`](examples/callgraph-ast.dl) | Call graph of dl's own source, this time via tree-sitter AST queries |
| [`callgraph-c.dl`](examples/callgraph-c.dl) | Resolved call graph for C, e.g. the Linux kernel. Same shape as |
| [`callgraph-resolved.dl`](examples/callgraph-resolved.dl) | Resolved call graph: A calls B only when a bare call B(...) appears INSIDE A's |
| [`callgraph-sg.dl`](examples/callgraph-sg.dl) | Same call graph as callgraph-ast.dl, but using ast-grep PATTERNS instead of |
| [`callgraph-typed.dl`](examples/callgraph-typed.dl) | Typed call graph: nodes are QUALIFIED names (Type::method), so two functions |
| [`callgraph.dl`](examples/callgraph.dl) | Call graph of dl's own source, discovered by pattern, queried as a graph. |
| [`cli-doc.dl`](examples/cli-doc.dl) | cli-doc.dl — keep the README CLI flag table fresh from the clap `Cli` struct |
| [`context-object.dl`](examples/context-object.dl) | context-object.dl — detect missing structs from local-name co-occurrence. |
| [`coupling-metrics.dl`](examples/coupling-metrics.dl) | coupling-metrics.dl — the dogfood instrument. dl measures engine.rs's OWN |
| [`dag-layers.dl`](examples/dag-layers.dl) | dag-layers.dl — longest-path topological tiering of the RA oracle file graph. |
| [`debug_lgg.dl`](examples/debug_lgg.dl) | debug_lgg.dl — inspect what type_edge rows exist for specific types |
| [`debug_type.dl`](examples/debug_type.dl) | debug_type.dl — check what's in type_edge and type_entity |
| [`debug_type_link.dl`](examples/debug_type_link.dl) | debug_type_link.dl — inspect type_link content |
| [`doc-coverage.dl`](examples/doc-coverage.dl) | doc-coverage.dl — the "undocumented API" rail, built on the doc spine. A |
| [`dup-collapse.dl`](examples/dup-collapse.dl) | dup-collapse.dl — RECOMMENDER, bootstrapped from measured tuples #1 and #2. |
| [`feature-envy.dl`](examples/feature-envy.dl) | feature-envy.dl — automatic refactor hints from the RA oracle. |
| [`field_matrix.dl`](examples/field_matrix.dl) | Extract the Engine method×field incidence matrix for spectral co-clustering. |
| [`flow-interproc.dl`](examples/flow-interproc.dl) | flow-interproc.dl — cross-function, SCIP-resolved value flow. |
| [`fn-graph.dl`](examples/fn-graph.dl) | fn-graph.dl — the 100%-recall function-level call graph from the RA oracle. |
| [`fuzzy-traits.dl`](examples/fuzzy-traits.dl) | Fuzzy latent traits over a Rust file (defaults to src/engine.rs). Last session's |
| [`gen-doc-index.dl`](examples/gen-doc-index.dl) | gen-doc-index.dl — dogfoods the doc tools on v5's own docs + source. |
| [`gen-engine-anchors.dl`](examples/gen-engine-anchors.dl) | gen-engine-anchors.dl — index dl's OWN language/engine features through dl's |
| [`gen-reference.dl`](examples/gen-reference.dl) | gen-reference.dl — programmable rustdoc/jsdoc for the engine's OWN surface. |
| [`gen-type-table.dl`](examples/gen-type-table.dl) | The marker-splice codegen loop on v5's own type graph: keep a fan-out table |
| [`gh-cache-batch.dl`](examples/gh-cache-batch.dl) | gh-cache-batch.dl — ghcacher's API-COST-AT-SCALE feature: the BATCHED PR sweep. |
| [`gh-cache-config.dl`](examples/gh-cache-config.dl) | gh-cache-config.dl — the reusable/configurable ghcacher: the watch set comes |
| [`gh-cache-full.dl`](examples/gh-cache-full.dl) | gh-cache-full.dl — the FULL ghcacher feature set as a datalog program, so the |
| [`gh-cache.dl`](examples/gh-cache.dl) | gh-cache.dl — ghcacher, as a datalog program. |
| [`glean.dl`](examples/glean.dl) | glean.dl — the "ask this codebase questions" showpiece, over v5's own source. |
| [`graph_score.dl`](examples/graph_score.dl) | graph_score.dl — TurboMQ (Mitchell & Mancoridis 2002) modularity scoring. |
| [`inspect_pairs.dl`](examples/inspect_pairs.dl) | inspect_pairs.dl — dump the field structure of specific LGG pairs |
| [`interface-soup.dl`](examples/interface-soup.dl) | Interface composition soup + over-abstraction smell, cross-language. |
| [`latest-turn-guardrail.dl`](examples/latest-turn-guardrail.dl) | Latest agent turn ∩ worktree change -> diag (for the LSP). |
| [`lint-dl-self.dl`](examples/lint-dl-self.dl) | lint-dl-self.dl — dl validates dl, scoped to what the agent just edited. |
| [`lint-docs.dl`](examples/lint-docs.dl) | Documentation hygiene lints. Two warnings: |
| [`lint-imports.dl`](examples/lint-imports.dl) | Broken-import linter: the module graph as a diagnostic source. |
| [`lint-no-touch.dl`](examples/lint-no-touch.dl) | No-touch guard: fence regions an agent must not hand-edit, and squiggle the |
| [`lint-unwrap.dl`](examples/lint-unwrap.dl) | Lint: flag `.unwrap()` outside test code. Run as a live linter: |
| [`lsp-def-target.dl`](examples/lsp-def-target.dl) | lsp-def-target.dl — go-to-def driven by a program-declared relation. |
| [`missing-repo.dl`](examples/missing-repo.dl) | Progressive multi-repo: a missing clone is non-fatal when its config row |
| [`missing-type.dl`](examples/missing-type.dl) | missing-type.dl — auto-detect "missing type" smells from local-name repetition. |
| [`module-history.dl`](examples/module-history.dl) | Rev-aware module graph. |
| [`node2vec-callgraph.dl`](examples/node2vec-callgraph.dl) | structural embedding of the v5 call graph |
| [`op-table.dl`](examples/op-table.dl) | op-table.dl — keep the README source-op table fresh from parse.rs dispatch. |
| [`openapi-lsp.dl`](examples/openapi-lsp.dl) | OpenAPI -> code cross-link. Demonstrates: (1) one spec extract (json wildcard |
| [`openapi.dl`](examples/openapi.dl) | OpenAPI coverage: which API operations have no frontend hook? |
| [`oracle-autopsy.dl`](examples/oracle-autopsy.dl) | oracle-autopsy.dl — where does sprefa's heuristic fail vs RA, in-scope? |
| [`oracle-check.dl`](examples/oracle-check.dl) | oracle-check.dl — RA (SCIP) vs sprefa (syn) at file granularity. |
| [`param-fan-out.dl`](examples/param-fan-out.dl) | param-fan-out.dl — god-fn signal: fns that declare many locals. |
| [`poll-head.dl`](examples/poll-head.dl) | Repo-HEAD watcher: poll each watched repo's git HEAD on an interval, cache the |
| [`rails-call-kind.dl`](examples/rails-call-kind.dl) | rails-call-kind.dl — the call_kind write-precision cut, as a standalone rail. |
| [`rails.dl`](examples/rails.dl) | Agent rails: checks scoped to the worktree diff, not the whole repo. |
| [`recall-lever.dl`](examples/recall-lever.dl) | recall-lever.dl — how much would a NAME-RESOLUTION pass over the ref spine |
| [`recall.dl`](examples/recall.dl) | recall.dl — fair recall of sprefa's FULL diet extraction vs RA's oracle. |
| [`recompute-guard.dl`](examples/recompute-guard.dl) | Static recompute-guard rail (sprefa over its own source). |
| [`refactor-clusters.dl`](examples/refactor-clusters.dl) | refactor-clusters.dl — refactor starting points from the RA-oracle graph, |
| [`refactor-discovery.dl`](examples/refactor-discovery.dl) | refactor-discovery.dl — refactor signals from the engine's resolved call graph. |
| [`refactor-init.dl`](examples/refactor-init.dl) | refactor-init.dl — from the 100%-recall SCIP oracle: where to START. |
| [`repo-nearest.dl`](examples/repo-nearest.dl) | Run with no --root from anywhere:  dl v5/examples/repo-nearest.dl |
| [`rtkq-op-recovery.dl`](examples/rtkq-op-recovery.dl) | RTK Query op-name recovery. RTKQ generates a hook identifier from each |
| [`rust.dl`](examples/rust.dl) | Rust lint pack — ast-grep patterns surfaced as LSP diagnostics. |
| [`string-fns.dl`](examples/string-fns.dl) | string-fns.dl — split / replace / computed binding / unary minus / NULL drop. |
| [`symbol-profile.dl`](examples/symbol-profile.dl) | symbol-profile.dl — the "ask about one symbol" view. Pin a symbol in |
| [`time.dl`](examples/time.dl) | The time axis: same pattern run against two revs of the tree, then an |
| [`ts.dl`](examples/ts.dl) | TypeScript/JS lint pack — ast-grep patterns surfaced as LSP diagnostics. |
| [`type_coincidence.dl`](examples/type_coincidence.dl) | type_coincidence.dl — which types co-occur in fn signatures. |
| [`type_lgg_query.dl`](examples/type_lgg_query.dl) | type_lgg_query.dl — filtered for actionable signal. |
| [`type_profile.dl`](examples/type_profile.dl) | type_profile.dl — per-type intelligence profile. |
| [`typegraph-anim.dl`](examples/typegraph-anim.dl) | Animated type-graph reveal: three d2 `steps:` boards, hottest hubs first. |
| [`typegraph.dl`](examples/typegraph.dl) | Self-hosted Rust type graph. |
| [`typeports.dl`](examples/typeports.dl) | Node-editor rendering of the type graph: each hub struct is a d2 sql_table |
<!-- END: examples-index -->

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
| [src/daemon.rs](src/daemon.rs) / [src/rpc.rs](src/rpc.rs) / [src/tray.rs](src/tray.rs) | warm-state daemon + spawn-if-missing client, JSON-RPC codec, menu-bar tray (see [docs/daemon.md](docs/daemon.md)) |
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
