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
- Ships an LSP (`sprefa-lsp`) + CLI (`sprefa-run`) + VS Code
  extension (`editors/vscode/`).

## Op inventory (landed)

| op          | body                  | what it does                                                                      |
|-------------|-----------------------|-----------------------------------------------------------------------------------|
| `repo`      | glob `or` `$NAME`     | filter on or bind `cursor.repo`                                                   |
| `rev`       | literal `or` `$NAME`  | filter on or bind `cursor.rev`; rejects wildcards                                 |
| `fs`        | glob `or` `$NAME`     | enumerate files under `(repo, rev)`; filter mode or bind mode                     |
| `read`      | *(none)*              | explicit byte-load. Optional — `comment`/`print`/etc. auto-load via `ensure_content_loaded` |
| `comment`   | regex `[, regex]`     | narrow `cursor.byte_range` to comment-marker regions (sequential or paired)       |
| `print`     | `[prefix]`            | emit `cursor.active()` as one line via `PrintEffect`                              |
| `str`       | raw bytes             | stash a constant byte slot                                                        |
| `void`      | *(none)*              | drop the cursor; fork-arm tail                                                    |
| `> $TARGET` | *(grammar-lowered)*   | capture-write: name `cursor.active()` as `$TARGET`                                |
| `rule(N)`   | single pipe or brace  | name a pipe or a group of pipes                                                   |

Parametric rules + sub-grammar ops (`ast[lang]`, `json`, `md`) are
specified in `parse.md` but not yet landed.

## Quick recipe

Create `hello.sprf`:

```sprf
# Find "sprefa-run" mentions inside comments under this tree.
fs(**/*.rs) > comment("sprefa-run") > print("match")
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
repo($R) > rev(HEAD) > fs($P)
# → emits one row per file, with R, P captures set.

# Filter (keep only matching cursors).
repo(myorg/*) > rev(HEAD) > fs(src/**/*.rs)
```

## Rules and rule-bodies

```sprf
rule(sources) {
  repo($R) > rev(HEAD) > fs(**/*.rs);
  repo($R) > rev(HEAD) > fs(**/*.toml);
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
v3/tests/smoke/_1_run.sh      # fs over .rs files
v3/tests/smoke/_2_rule.sh     # brace-body sub-pipes
v3/tests/smoke/_3_comment.sh  # fs > read > comment > print end-to-end
```

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
  — one source exercising every §8–§13 syntax node: `rule(…)`,
  `ast[lang]{…}`, `&.$X` cursor-ref, `${target.$FIELD > $T}`
  xref capture, `sh{…}` effect, `tag(:repo, $R)`.

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
