# sprefa v3

Cross-codebase causal-linking engine. v3 is the active line; the v2
crate in the sibling `v2/` directory still runs but new work lands here.

Language spec: [`crates/sprefa_parse/parse.md`](crates/sprefa_parse/parse.md).
Perf + framework notes: [`docs/FINDINGS.md`](docs/FINDINGS.md).

## What v3 does today

- Parses `.sprf` source into pipes with a tree-sitter host grammar +
  per-op injected sub-grammars (glob, regex).
- Runs pipes through a reactive runner built on top of
  `effect_runtime` (pure-effect cache + cancellation + batching).
- Static `BindingGraph` analysis for `${TERM}` reads / `${TERM?}`
  introducers; surfaces `term/unbound` and `term/shadowed` warnings
  through the LSP.
- Ships an LSP (`sprefa-lsp`) + CLI (`sprefa-run`) + VS Code
  extension (`editors/vscode/`).

## Status (2026-04-25)

v3 is in maintenance / use-case-driven mode. The op stdlib + grammar +
runtime + LSP are stable enough to write real verification rules
against checkouts of multiple repos. New ops land when a concrete use
case demands them (target candidates: `json`, `tag`, two-pass diff
between revs). Speculative architecture work (source-op arg-pipe
unification, term-as-Subject runtime, flow-class taxonomy, pluggable
DSL delimiters) is documented in
[`../chat_log/20260425.2.sprefa-4iv-ambiguities.md`](../chat_log/20260425.2.sprefa-4iv-ambiguities.md)
and is on hold.

## Op inventory (landed)

| op        | body                  | what it does                                                                      |
|-----------|-----------------------|-----------------------------------------------------------------------------------|
| `repo`    | glob `or` `$NAME`     | filter on or bind `cursor.repo`                                                   |
| `rev`     | `:atom` `or` `$NAME`  | filter on or bind `cursor.rev`; rejects wildcards                                 |
| `fs`      | `glob(...)` `or` `$NAME` | enumerate files under `(repo, rev)`; filter mode or bind mode                  |
| `read`    | *(none)*              | explicit byte-load. Optional — `comment`/`print`/etc. auto-load via `ensure_content_loaded` |
| `comment` | regex `[, regex]`     | narrow `cursor.byte_range` to comment-marker regions (sequential or paired)       |
| `print`   | `[prefix]`            | emit `cursor.active()` as one line via `PrintEffect`                              |
| `str`     | raw bytes             | stash a constant byte slot                                                        |
| `void`    | *(none)*              | drop the cursor; fork-arm tail                                                    |
| `glob`    | glob pattern          | pattern op exposing a regex; consumed by `fs` / `repo` arg slots                  |
| `re`      | regex pattern         | pattern op exposing a regex; consumed by `fs` / `repo` arg slots                  |
| `rule(N)` | single pipe or brace  | name a pipe or a group of pipes                                                   |
| `tag`     | `(:r, $X, $Y)`        | append/read rows in a relational bag; `tag?` adds predicate / probe / join         |
| `rule`    | `(:r, ${A?}, ...) {}` | parametric rule definition; body terminal cursors sink as relation rows           |

Parametric rules + sub-grammar ops (`ast[lang]`, `json`, `md`) are
specified in `parse.md` but not yet landed.

## SQLite drain (end-of-run)

`sprefa-run` writes every rule's terminal-cursor rows to a SQLite
database at exit. Default db path is `<sprf_stem>.db` next to the
`.sprf` source; override with `--out <path>`.

Per-rule schema (v0/v1 parity, flat shape):

- table `<sprf_stem>__<rule_name>`
- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- synthetic columns from cursor fields lead: `repo`, `rev`, `fs`
- one paired (`<cap>`, `<cap>_norm`) `TEXT` per distinct user capture
- `_norm` is `sprf_norm()` (strip non-ASCII-alphanumeric + lowercase),
  ported 1:1 from `crates/config/src/normalize.rs`
- `CREATE INDEX <table>_<col>_norm_idx` per normalized column
- FTS5 trigram virtual table `<table>_fts` indexing every `_norm`
  column with `content=` link + ai/ad/au sync triggers
- pool PRAGMAs: `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON`

Tables drop+recreate each run. Incremental caching (sprf_meta +
content_hash + scanner_hash) is filed under bd `sprefa-upb` and not
yet landed; `norm2` (configurable suffix-strip) under `sprefa-9ma`;
shared `strings`/`refs`/`files` dedup tier filed separately.

## Quick recipe

Create `hello.sprf`:

```sprf
# Find "sprefa-run" mentions inside comments under this tree.
fs(glob(**/*.rs)) > comment("sprefa-run") > print("match")
```

`fs` lists paths; `comment` + `print` auto-load bytes on demand via
the shared `ensure_content_loaded` helper. An explicit `> read` step
is only needed to force-load at a specific point (e.g. for a custom
op that does not call the helper yet).

Run it:

```bash
cd v3
cargo build -p server --bin sprefa-run
./target/debug/sprefa-run hello.sprf --root crates/server --rev HEAD
```

Output: one `match: <line>` per comment region hit, with a trailing
`<header> — N rows` summary.

## Binding and filtering

```sprf
# Bind (synthesized capture written into cursor).
repo($R) > rev(:HEAD) > fs(glob(**/$P.rs))
# → emits one row per file, with R, P captures set.

# Filter (keep only matching cursors).
repo(myorg/*) > rev(:HEAD) > fs(glob(src/**/*.rs))
```

## Rules and rule-bodies

```sprf
rule(sources) {
  repo($R) > rev(:HEAD) > fs(glob(**/*.rs));
  repo($R) > rev(:HEAD) > fs(glob(**/*.toml));
}
```

A brace body lowers to one sub-pipe per `;`, each run under the rule's
name. Output rows are tagged with `rule sources pipe 0 — …`.

## Config file

Seeds can live in `.sprefa.toml` instead of CLI flags:

```toml
[[seeds]]
slug = "server"
root = "crates/server"
rev  = "HEAD"
```

```bash
./target/debug/sprefa-run hello.sprf --config .sprefa.toml
```

## VS Code extension

`editors/vscode/` ships syntax highlighting + an LSP client that talks
to the `sprefa-lsp` binary built from this workspace.

### One-shot install

```bash
editors/vscode/install.sh           # debug build
editors/vscode/install.sh --release # or optimized
```

The script:

1. Builds `sprefa-lsp` via `cargo build -p server --bin sprefa-lsp`.
2. Runs `npm install` + `tsc` in `editors/vscode/`.
3. Packages a local `.vsix` with `vsce`.
4. Installs it with `code --install-extension … --force`.
5. Writes the absolute path of the built binary into VS Code user
   settings under `sprf.serverPath`, so the client never falls back
   to a random `sprefa-lsp` from `$PATH`.

Re-runs are idempotent — rebuilding after a Rust change is just
`editors/vscode/install.sh` again. Reload any open VS Code window
afterward to pick up the new binary.

Prerequisites: `code`, `vsce`, `node`/`npm`, `cargo`. On macOS:

```bash
brew install --cask visual-studio-code
npm install -g @vscode/vsce
```

### Uninstall

```bash
code --uninstall-extension sprefa.sprf
```

### What the extension surfaces today

Hover on any op name (`repo`, `fs`, `read`, `comment`, `print`, …) to
see its Registry doc. Hover inside injected pattern bodies
(glob, regex) is planned; the host-name path is live.

## Smoke tests

```bash
v3/tests/smoke/_1_run.sh             # fs over .rs files
v3/tests/smoke/_2_rule.sh            # brace-body sub-pipes
v3/tests/smoke/_3_comment.sh         # fs > comment > print end-to-end
v3/tests/smoke/_4_fs_glob_nested.sh  # fs(glob(...)) nested-arg form
v3/tests/smoke/_5_rev_atom_filter.sh # rev(:HEAD) atom filter
```

All five pass against `crates/server` itself as the seed root.

## Fixtures and examples

Landed, runnable today:

- [`crates/server/fixtures/smoke.sprf`](crates/server/fixtures/smoke.sprf)
  — minimal `repo > rev > fs` bind.
- [`crates/server/fixtures/rule_smoke.sprf`](crates/server/fixtures/rule_smoke.sprf)
  — `rule(sources) { ... }` with two sub-pipes.
- [`crates/server/fixtures/comment_smoke.sprf`](crates/server/fixtures/comment_smoke.sprf)
  — full content-contract walk: `fs > read > comment > print`.

Parse-only (host CST coverage, no runtime):

- [`crates/tree-sitter-sprefa/tests/kitchen_sink.rs`](crates/tree-sitter-sprefa/tests/kitchen_sink.rs)
  — one source exercising the locked syntax surface: `rule(…) { … }`,
  `ast[lang]{…}` injected pattern body, `${TERM?}` brace-mandatory
  unbound term, `${TERM}` brace-mandatory bound read, `${{…}}` shell
  literal, `tag(:atom, $TERM)`. Cursor-ref / xref / capture-write
  retired (sprefa-r6k); brace-mandatory terms locked (sprefa-9lt).

Prior-art kitchen sinks (reference only; they target v1/v2 surface and
exercise ops that v3 has not yet ported):

- [`crates/sprf/tests/fixtures/kitchen_sink.sprf`](../crates/sprf/tests/fixtures/kitchen_sink.sprf)
  — v1 full feature sweep: `fs`, `json`, `folder`, scoped blocks,
  recursive descent, scan-pointer joins, cross-repo refs.
- [`crates/sprf-lsp/tests/e2e/fixtures/kitchen_sink.sprf`](../crates/sprf-lsp/tests/e2e/fixtures/kitchen_sink.sprf)
  / [`kitchen_sink_v2.sprf`](../crates/sprf-lsp/tests/e2e/fixtures/kitchen_sink_v2.sprf)
  — LSP diagnostics / hover fixtures for v1 and v2.

## Build and test

```bash
cd v3
cargo test                               # full workspace
cargo test -p pipeline --lib             # fast lib tests
cargo build -p server --bin sprefa-run
cargo build -p server --bin sprefa-lsp
```

## Layout

```
v3/
├── Cargo.toml
├── crates/
│   ├── effect_runtime/        framework: EffectKind, PureEffect, Batcher, RtCtx
│   ├── pipeline/              Op trait, Cursor, registry, ops, effects
│   ├── sprefa_parse/          host tree-sitter parse + parse.md spec
│   ├── sprefa_macros/         proc-macros (pattern-op derive)
│   ├── server/                sprefa-run CLI + sprefa-lsp + DocSession
│   └── tree-sitter-sprefa/    host grammar (grammar.js, parser.c)
├── experiments/               effect_proof benches, lang_prototype
├── docs/                      FINDINGS, PRIOR_ART, spec side-files
└── tests/smoke/               end-to-end shell scripts
```

## Further reading

- `crates/sprefa_parse/parse.md` — language spec. §14.5 covers each
  op's sub-grammar contract; §14.5i-m cover the ops added this week.
- `docs/FINDINGS.md` — perf measurements and the rules they support.
- `docs/PRIOR_ART.md` — Haxl / tower / salsa / redux-saga survey.
