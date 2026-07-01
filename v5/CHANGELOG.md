# Changelog

All notable changes to `dl` (sprefa v5) are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions track the `v*` release
tags consumed by cargo-dist.

## [Unreleased]

## [0.2.0] - 2026-07-01

### Added
- **Named args + field punning on relation atoms.** A body atom or `?` query may
  pass args by declared column: `type_edge(from: f, kind: "impl")`. Once any
  `col:` appears the atom is in named mode, where a bare identifier puns to its
  own column (`from` == `from: from`, the JS/Rust-struct shorthand), and any
  unmentioned column is a don't-care — so you name only the columns you use
  instead of counting positional `_`. Resolution rides the relation's declared
  columns (user `rel` decls and built-in schemas alike) in a frontend pass, so it
  works across a forward reference. Positional atoms are unchanged. Named args in
  a rule head are rejected for now (aggregate interaction deferred).
- **`dl update` — self-update to the latest release.** Re-runs the cargo-dist
  installer for the newest tag; `--check` reports the installed vs latest version.
- **`dl index` — turnkey SCIP generation.** Detects the language(s) at a root by
  marker file (Cargo.toml / tsconfig / package.json / pyproject / go.mod /
  build.gradle / pom.xml / compile_commands.json / CMakeLists), runs the matching
  indexer (rust-analyzer, scip-typescript, scip-python, scip-go, scip-java,
  scip-clang), and places the result at `<root>/.dl/index.scip`. `--install` runs
  the per-indexer install command; `--rev REV` prints the worktree-and-index
  recipe (SCIP covers the working tree only). A polyglot workspace produces one
  merged index via `scip_import::merge_files`.
- **`dl doctor` — SCIP health screen.** Reports detected languages, indexer
  availability, index presence + freshness (mtime vs HEAD), path-join sanity, and
  `scip_*` row counts. Turns each formerly-silent SCIP failure into a visible line.

### Changed
- The SCIP importer auto-loads `<root>/.dl/index.scip` in addition to
  `$SPREFA_SCIP_INDEX` and `<root>/index.scip`, so a `dl index`-generated index is
  found with no configuration. `dl index` appends `index.scip*` to
  `.dl/.gitignore`, so a generated index (often 100MB+) never lands in git.
- The indexer always runs with `cwd = root`, so SCIP `relative_path` keys join the
  paths the scanners see (removes the silent-empty-from-wrong-dir failure mode).

### Fixed
- **Undeclared head relation is a clear diagnostic, not a SQLite leak.** A rule or
  `?` query over a relation with no `rel` decl now reports `unknown-relation`
  (through `--check`/LSP) naming the relation, instead of failing at execution as
  a raw `no such table: rel_X`.
- **Independent `?` queries.** A query that fails at evaluation (e.g. wrong arity)
  reports its own failure and no longer aborts the rest of the query chain.
- **Zero-match `scan` warns.** A source rule whose glob matches no files prints a
  warning naming the rule, glob, and `repo@rev (root)` it looked under, instead of
  silently producing 0 rows downstream.
- **A bare `//` gives a clear message** ("dl comments start with `#`") instead of a
  baffling `Regex("")` parse error.

### Guardrails
- SCIP generation is explicit and single-root only. Nothing (daemon, reload gate,
  `scan("*")` fan-out) generates an index automatically; the daemon only imports
  one that already exists. `dl index` refuses an aggregation directory — the XDG
  serving home, or a folder containing nested git repos — unless `--force`, so on
  a machine whose daemon watches hundreds of repos a stray marker file cannot turn
  one command into hundreds of indexer runs.
