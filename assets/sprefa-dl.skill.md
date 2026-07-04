---
name: sprefa-dl
description: Use the `dl` engine (sprefa v5) to query a codebase as datalog-over-code — call graphs, import/type graphs, blast radius, lint rails, and codemods. Reach for this when an agent needs structured facts about code (who calls X, what imports Y, what breaks if Z changes) instead of grepping, or wants a reactive .dl rule/LSP diagnostic/codemod.
---

# sprefa `dl` — datalog over code

`dl` extracts facts from source (`scan` + `match`/`ast`/`sg`/`json`), lets you
write recursive datalog rules, and lowers them to a SQLite fixpoint. Output is a
`?` query, an LSP/`--check` diagnostic, an MCP response, or a generated/spliced
file (`gen`). Every fact is keyed on `(repo, path, rev)`; matched values carry
byte spans, so a match is a coordinate you can squiggle or rewrite.

## Before you write a rule (the constraints that bite)

Read these first; each one is a silent zero-match or a confusing error otherwise.
Run `dl prog.dl --parse-only` to parse + typecheck with NO scan (sub-second fast
fail) before a full run.

- **Captures are UPPERCASE.** In `sg`/`ast_yaml` patterns a metavar is `$NAME`
  or `$$$ARGS`; a lowercase `$name` is matched as LITERAL text, not a capture
  (a warn fires). Descriptive-but-uppercase: `$CALLEE`, `$$$CALL_ARGS`.
- **`$$$NAME` is structure only.** A `$$$` multi-metavar matches a node LIST; it
  binds no head var. Bind a single node with `$NAME`, or read the span outputs.
- **Arithmetic is head/comparison only.** `line + 1` is legal in a rule head
  (`next_line(scan_path, line + 1) <- ...`) or on a comparison side, never as a
  body binding. To COMPUTE a value, put the expression in the head, not `l = m+1`.
- **Regexes are Rust-flavor.** No lookahead/lookbehind (`(?!...)`), no backrefs.
  Anchor with `^`, `$`, `\b`, and character classes instead.
- **`comment` sees whole-line comments only.** A trailing inline `// ...` after
  code is invisible to the `comment` op; match a whole-line marker.
- **Anchor disjunct markers.** In an alternation, a shorter marker substring-
  matches a longer one (`dl-disable` inside `dl-disable-next-line`); anchor each
  branch (`^`, `\b`, trailing `$`) so they do not collide.
- **`scan` has two optional leading args** — copy-paste one of these two forms:
  - with repo: `scan(config_repo, rev, "src/**/*.rs", scan_path, rev_out)`
  - without:   `scan("src/**/*.rs", scan_path, rev_out)`
- **Pick the op by whether a grammar exists.** `sg`/`ast`/`ast_yaml` when the
  language has a grammar (below); `match` for substrings or a language with no
  grammar handle. Do not reach for `match` on a structural pattern.
- **Embedded languages: TERM-form `sg`.** `sg(:lang, str_var, "pat"[, line, col,
  end_line, end_col])` runs the ast-grep grammar over a STRING bound earlier in
  the rule instead of a file — a styled-components css body captured by an outer
  `sg(:tsx)`, a markdown code fence, a response column. Spans are RELATIVE to the
  bound string (byte 0 = its start; carry the region's own line to reach file
  coordinates). Runs in the join+extract pass like term-form `json`/`jsonp`, so it
  heads its own rel (never co-headed with a derived rule). See
  `examples/styled-components.dl` and `examples/md-fences.dl`.
- **One rel = one rule kind.** Never head a rel with BOTH a source rule
  (`scan`/`match`/`ast`/`sg`/`ast_yaml`/`json`/`cmd`/`comment`) and a derived
  rule — the engine bails. Split into two rels, union in a third.
- **A `closure`/`scc`/`node2vec` rel can't be read unpinned in a rule body.** The
  recursive view materializes; seed a recursive rule instead, or pin both ends of
  a `?` query (`? reaches("a", x).`).

### Per-op language matrix

A `:lang` used outside its op's list bails at scan time — `sg`/`ast_yaml` share
the ast-grep grammars (they have `tsx`/`typescript`/`cpp`), while `ast` runs
tree-sitter (it has `bash`/`hcl`/`gotmpl`/`dockerfile` but NO `tsx`). The real
tables live in `src/sg.rs` (`SG_LANG_TABLE`) and `src/engine/mod.rs`
(`AST_LANG_TABLE`); a test keeps this block set-equal to them:

    sg, ast_yaml: rust typescript tsx javascript python go json c cpp kotlin css html bash csharp java scala swift ruby php lua elixir haskell yaml
    ast: rust c kotlin python bash go hcl starlark jsonnet gotmpl dockerfile

## Install

```sh
# from a local clone (the crate lives at the repo root; run from there)
cargo install --path . --force
# or from git
cargo install --git https://github.com/hafley66/sprefa sprefa-dl
```

Turnkey, post-download (the skill text + starter `.dl` are embedded in the
binary, so this works on a prebuilt download with no source tree):

```sh
dl setup                 # install this skill + wire detected agents (Claude Code, opencode)
dl setup --project .     # bootstrap a repo: .dl/ rail + AGENTS.md/CLAUDE.md section
dl setup --print         # dump the embedded skill to stdout
```

From source, `install-dl.sh` also `cargo install`s the binary first.

## Discover examples + reusable libs (embedded in the binary, no disk)

The `.dl` corpus and the `std/` libs are baked into the binary, so these work on
a bare prebuilt download:

```sh
dl examples                      # list every embedded example (name + summary)
dl examples scip dataflow        # semantic search (cosine, offline stub embedder)
dl examples --show openapi-lsp   # print one to stdout (read/load without disk)
dl examples --std                # the use-able std libs
dl <(dl examples --show openapi-lsp) --root .   # run an embedded example directly
```

Reusable tools: `use "std/callgraph.dl".` resolves from disk if present, else
from the embedded copy — a stdlib that works with no source tree. (Real semantic
search: build with `--features embed-fastembed` to swap the stub for an ONNX model.)

## Three ways to run

| mode | command | when |
|---|---|---|
| ad-hoc | `dl prog.dl --root .` | one-off query, prints `?` rows |
| discovery | `dl --check --root <repo>` | runs every `<repo>/.dl/*.dl`; exit 2 on any `diag` row (CI rail) |
| LSP | `dl prog.dl --root <repo> --lsp` | live editor diagnostics from `diag` rows |

`--root` defaults to the nearest `.git` ancestor. Add `--no-daemon` for an
isolated ad-hoc run (the daemon otherwise hijacks ad-hoc invocations).

## Rule heads: named args, kwargs, bare puns (v0.4.0)

Once ANY atom carries a `col: term`, the whole atom is in named mode: a term
that carries a name binds by name, a nameless term binds by position. A bare
`Var` puns to its own column (`edge(from, to)` == `edge(from: from, to: to)`),
and an unmentioned column is left `NULL`/don't-care. Works in rule heads too,
so a fixed-schema sink like `diag` (9 columns) is written by naming only the
columns you use:

```dl
rel import_edge(importer_path: text, imported_path: text, line: int).

diag(path: importer_path, line: line, severity: "warning", code: "long-import-chain",
     msg: "import chain worth checking") <-
    import_edge(importer_path, imported_path, line), line > 5.
```

## Lattice relations: `key(...)` + `merge(...)`

`key` names a conflict target narrower than the whole row (a choice-domain,
Soufflé-style); `merge` resolves a collision instead of silent-picking the
first write. Used for "one winner per id": a dispatch table, a lint severity
ceiling, an etag carry.

```dl
rel candidate(sym: text, score: int).
rel best(sym: text, score: int) key(sym) merge(MaxBy(score)).
best(sym, score) <- candidate(sym, score).
```

## Ports (`@in`/`@out`) + `--mcp`

`rel req(id: text, method: text, params: text) @in(rpc).` marks a rel a PORT:
`class` (`rpc`) names a fixed contract, never a transport, and the column
envelope is checked by NAME at declare time (`rpc` in = `id`/`method`/`params`,
out = `id`/`result`). `--mcp` binds `rpc` ports to stdio × JSON-RPC; the same
program could serve HTTP later unchanged. A rule/fact heading an `@in` port is
rejected: the serving loop is the only writer. See `examples/mcp-echo.dl`
(lattice dispatch) and `examples/mcp-server.dl` (tools/list + tools/call).

## Effect templates: `sh` / `sh!` / `sh*` + `collect()`

Content-addressed on `(head_rel, args)` so an unchanged request never re-fires:
`sh` = read effect (at-least-once retry), `sh!` = mutate effect (idempotency-
keyed, claimed and run exactly once), `sh*` = stream effect (each stdout line
fans into its own row). `collect(x, n)` batches every solution for `x` into one
request capped at `n` per batch, the ghcacher N-calls-become-1 win.

```dl
rel watched_endpoint(endpoint: text).

sh fetch_endpoint(endpoint) -> (status_code: int, body_text: text) =
  `curl -s -o /dev/null -w "%{http_code}" {endpoint}`.

rel endpoint_status(endpoint: text, status_code: int, body_text: text).
endpoint_status(endpoint, status_code, body_text) <-
    @async watched_endpoint(endpoint), fetch_endpoint(endpoint) -> (status_code, body_text).
```

`examples/gh-cache.dl` (etag-carry `sh`), `examples/npm-crawl.dl` (`sh*`),
`examples/gh-cache-batch.dl` (`collect`).

## When to reach for it (instead of grep)

- **Call graph / blast radius**: who calls X, what transitively reaches Z —
  `closure(call_edge)`, `reaches(Term, X)`.
- **Import graph**: `module_edge`, broken imports (`examples/lint-imports.dl`).
- **Type graph**: `type_edge(from, to, kind)`, `type_entity`/`type_sig`/`type_link`
  (SCIP-resolved sym→sym), cross-language (Rust/TS/Kotlin).
- **Dataflow**: `df_node`/`df_edge`/`df_arg`/`df_param`/`df_field`, unioned into
  `flow_edge` by `use "std/flow.dl".` (`examples/flow-interproc.dl`, `taint.dl`).
- **Lint rails**: a rule heading `diag(path, line, col, ..., severity, code, msg)`
  becomes a `--check`/`--lsp` gate (`examples/lint-*.dl`).
- **Codemods**: `--move OLD=NEW` rewrites import paths via the located span spine.
- **SCIP**: set `SPREFA_SCIP_INDEX` (or drop `index.scip` at root) to load
  compiler-backed `scip_def`/`scip_ref`/`scip_edge`.

## Extraction-family built-in relations

Each family populates lazily the first time a program references one of its
relations (a bare `scan` seeds the file set; referencing e.g. `type_entity`
opts type extraction in over that scan, no separate enable step). Full
schemas: `docs/reference/relations.md` (generated, grouped by `group`).

| family | key relations | opts in via |
|---|---|---|
| type | `type_entity`, `type_sig`, `type_link`, `type_edge`(`_rev`) | any `type_*` atom |
| call | `call_def`, `call_site`, `call_edge`, `call_name` | any `call_*` atom |
| dataflow | `df_node`, `df_edge`, `df_arg`, `df_param`, `df_field` | any `df_*` atom |
| doc | `doc_comment`, `doc_tag` | either atom |
| module | `module_edge`, `module_node` | any `module_*` atom |

Telemetry as facts: `rel_count(rel, rows)` and `stmt_ms(rel, ms)` report the
last tick's own row counts and per-statement wall time; `examples/perf-rails.dl`
turns them into a row-budget / slow-rule `diag` rail.

## `--hook`: Claude Code PostToolUse hook, as a rule

`dl --hook` reads a Claude Code hook event (PostToolUse JSON) on stdin, ticks
the program, and emits `inject`/`inject_skill`/`block` rows as the hook's
output: no editor, no bash, the condition is a rule. `dl setup --project .`
wires the `.claude/settings.json` registration.

```dl
rel inject_skill(skill_name: text).

inject_skill("testing") <-
    changed(changed_path), changed_path =~ /(_test\.|\.test\.|\.spec\.)/,
    !skill_loaded(_, _, "testing").
```

## Self-documenting (read these, don't guess the surface)

The engine generates its own reference from self-describing catalogs
(`rel_catalog`, `fn_catalog`, `op_catalog`) via `examples/gen-reference.dl`:

- `docs/reference/relations.md` — every built-in relation
- `docs/reference/functions.md` — every scalar function
- `docs/reference/syntax.md` — every source/body/sink op (syntax + semantics)
- `docs/reference/examples.md` — the `examples/` corpus, one line each
- `README.md` — the full human+agent reference

Regenerate after touching the engine: `dl examples/gen-reference.dl --root .`
(repo root, not `v5/`, see Install). Docs are spliced/convergent; never
hand-edit inside `<!-- BEGIN -->`/`<!-- END -->`.

### Op quick-reference

Every source/body/sink op, spliced from `op_catalog` by
`examples/gen-skill-ref.dl` (do not hand-edit between the markers). That same
program is a `--check` freshness rail: it fails if this skill names a relation or
op that no longer resolves in a catalog.

<!-- BEGIN: op-quickref -->
| op | kind | syntax |
|---|---|---|
| `aggregation` | body | `count sum min max` |
| `arith` | body | `+ - * / %` |
| `ast` | source | `ast(path, rev, :lang, "(query) @cap", line[, end])` |
| `ast_yaml` | source | `ast_yaml(path, rev, :lang, "rule yaml", line, ...)` |
| `atom` | body | `edge(from, to) / edge(to: dst) / edge("x", 1, kind: edge_kind)` |
| `closure` | body | `closure(edge)` |
| `cmd` | source | `cmd(path, rev, "tool {file}", line, out)` |
| `comment` | source | `comment(path, rev, /open/[, /close/], l0, l1, label)` |
| `comparison` | body | `= != < <= > >=` |
| `diag` | sink | `diag(path: hit_path, line: hit_line, msg: message[, col: , end_line: , end_col: , severity: , code: , hint: ]) <- ...` |
| `gen` | sink | `gen([:mode,] path, [l0, l1,] "{var} template")` |
| `glob` | body | `path ~~ "src/*"` |
| `json` | source | `json(path, rev, q:{ $k: $v })` |
| `jsonp` | source | `jsonp(path, rev, "a.*.b", out)` |
| `match` | source | `match(path, rev, /re/, line[, id][, col, end_col])` |
| `negation` | body | `!edge(from, _)` |
| `node2vec` | body | `head(node_a, node_b, score) <- node2vec(edge)` |
| `query` | sink | `? rel(from, to). / ? rel(col: value).` |
| `regex` | body | `name =~ /^[A-Za-z]+$/` |
| `scan` | source | `scan([repo,][rev,] glob, path[, rev_out])` |
| `scc` | body | `head(rep, member) <- scc(edge)` |
| `sg` | source | `sg(path, rev, :lang, "$X.unwrap()", line[, col, end_line, end_col][, id])` |
| `strfn` | body | `split(text, sep, idx) / replace(text, from, to)` |
<!-- END: op-quickref -->

## Self-validating (dl lints dl, like rust-analyzer)

`dl_diag(path, line, col, end_line, end_col, severity, code, msg)` runs the
engine's own lex/parse/typecheck over every scanned `.dl` file. Build a rail:

```dl
rel agent_changed(changed_path: text).
agent_changed(changed_path) <- agent_touch(_, _, changed_path).

diag(path: changed_path, line: line, col: col, end_line: end_line, end_col: end_col,
     severity: severity, code: code, msg: msg) <-
    agent_changed(changed_path), changed_path =~ /\.dl$/,
    dl_diag(changed_path, line, col, end_line, end_col, severity, code, msg).
```

`examples/lint-dl-self.dl` is this rail; `dl --check` exits 2 on a broken `.dl`.

## Git-free agent relations

`agent_edit` / `agent_touch` read the harness session store keyed on the
`--root` DIRECTORY — no git, the file need not be tracked or committed. Only
`changed` / `changed_line` / `created` need a git repo (empty outside one).

## Authoring gotchas

- **N+1**: never a per-row write. Collect the set, one `insert_rows`/`refresh_rel`.
- **One rel = one rule kind**: never head a rel with both a source rule
  (`scan`/`match`/`ast`/`sg`/`json`/`cmd`/`comment`) and a derived rule — the
  engine bails. Split and union in a third rel.
- **Reserved names**: `repo`, `rev`, `content`, `file`, `string`, `ref`, the
  `type_*`/`call_*`/`df_*`/`doc_*`/module-graph families, `dl_diag`, plus every
  relation any built-in family owns; pick another name (anim uses `node_ref`).
- **Banned identifiers**: `provenance`→source, `substrate`→base,
  `load-bearing`→critical, `regime`→mode.

## Adding a built-in relation

See the `sprefa-v5-new-builtin-rel` skill. Short version (current pattern):
a `RelKind` impl in `src/rels/` + register in `rel_kinds()`, with `group`/`doc`
set on each `RelDecl` (an empty doc fails the doc-completeness test; the
catalog + generated README table read the decl). The reserved-name guard, both
tick paths, and decls wire automatically.
