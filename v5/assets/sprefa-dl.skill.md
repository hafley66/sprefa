---
name: sprefa-dl
description: Use the `dl` engine (sprefa v5) to query a codebase as datalog-over-code — call graphs, import/type graphs, blast radius, lint rails, and codemods. Reach for this when an agent needs structured facts about code (who calls X, what imports Y, what breaks if Z changes) instead of grepping, or wants a reactive .dl rule/LSP diagnostic/codemod.
---

# sprefa `dl` — datalog over code

`dl` extracts facts from source (`scan` + `match`/`ast`/`sg`/`json`), lets you
write recursive datalog rules, and lowers them to a SQLite fixpoint. Output is a
`?` query, an LSP/`--check` diagnostic, or a generated/spliced file (`gen`).
Every fact is keyed on `(repo, path, rev)`; matched values carry byte spans, so
a match is a coordinate you can squiggle or rewrite.

## Install

```sh
# from a local clone (run from the repo root)
cargo install --path v5 --bin dl --force
# or from git
cargo install --git https://github.com/hafley66/sprefa sprefa-dl --bin dl
```

Turnkey, post-download (the skill text + starter `.dl` are embedded in the
binary, so this works on a prebuilt download with no source tree):

```sh
dl setup                 # install this skill + wire detected agents (Claude Code, opencode)
dl setup --project .     # bootstrap a repo: .dl/ rail + AGENTS.md/CLAUDE.md section
dl setup --print         # dump the embedded skill to stdout
```

From source, `v5/install-dl.sh` also `cargo install`s the binary first.

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

## When to reach for it (instead of grep)

- **Call graph / blast radius**: who calls X, what transitively reaches Z —
  `closure(call_edge)`, `reaches(Term, X)`.
- **Import graph**: `module_edge`, broken imports (`examples/lint-imports.dl`).
- **Type graph**: `type_edge(from, to, kind)`, `type_entity`/`type_sig`/`type_link`
  (SCIP-resolved sym→sym), cross-language (Rust/TS/Kotlin).
- **Lint rails**: a rule heading `diag(path, line, col, ..., severity, code, msg)`
  becomes a `--check`/`--lsp` gate (`examples/lint-*.dl`).
- **Codemods**: `--move OLD=NEW` rewrites import paths via the located span spine.
- **SCIP**: set `SPREFA_SCIP_INDEX` (or drop `index.scip` at root) to load
  compiler-backed `scip_def`/`scip_ref`/`scip_edge`.

## Self-documenting (read these, don't guess the surface)

The engine generates its own reference from self-describing catalogs
(`rel_catalog`, `fn_catalog`, `op_catalog`) via `examples/gen-reference.dl`:

- `v5/docs/reference/relations.md` — every built-in relation
- `v5/docs/reference/functions.md` — every scalar function
- `v5/docs/reference/syntax.md` — every source/body/sink op (syntax + semantics)
- `v5/docs/reference/examples.md` — the `examples/` corpus, one line each
- `v5/README.md` — the full human+agent reference

Regenerate after touching the engine: `dl examples/gen-reference.dl --root v5`.
Docs are spliced/convergent; never hand-edit inside `<!-- BEGIN -->`/`<!-- END -->`.

## Self-validating (dl lints dl, like rust-analyzer)

`dl_diag(path, line, col, end_line, end_col, severity, code, msg)` runs the
engine's own lex/parse/typecheck over every scanned `.dl` file. Build a rail:

```
agent_changed(p) <- agent_touch(_, _, p).
diag(p, l, c, el, ec, sev, code, msg) <-
    agent_changed(p), p =~ /\.dl$/, dl_diag(p, l, c, el, ec, sev, code, msg).
```

`examples/lint-dl-self.dl` is this rail; `dl --check` exits 2 on a broken `.dl`.

## Git-free agent relations

`agent_edit` / `agent_touch` read the harness session store keyed on the
`--root` DIRECTORY — no git, the file need not be tracked or committed. Only
`changed` / `changed_line` / `created` need a git repo (empty outside one).

## Authoring gotchas

- **N+1**: never a per-row write. Collect the set, one `insert_rows`/`refresh_rel`.
  The tick counter screams otherwise.
- **One rel = one rule kind**: never head a rel with both a source rule
  (`scan`/`match`/`ast`/`sg`/`json`/`cmd`/`comment`) and a derived rule — the
  engine bails. Split and union in a third rel.
- **Reserved names**: `repo`, `rev`, `content`, `file`, `string`, `ref`,
  `type_edge`/`type_edge_rev`/`type_entity`/`type_sig`/`type_link`, the
  module-graph rels, `dl_diag`. Pick another name (anim uses `node_ref` for a
  ref-shaped rel).
- **Banned identifiers**: `provenance`→source, `substrate`→base,
  `load-bearing`→critical, `regime`→mode.

## Adding a built-in relation

See the `sprefa-v5-new-builtin-rel` skill. Short version (current pattern):
a `RelKind` impl in `relkind.rs` + register in `rel_kinds()` + a
`builtin_rel_docs()` entry. The reserved-name guard, both tick paths, and decls
wire automatically. (The older `*_RELS`/`refresh_*_rel` engine.rs pattern is stale.)
