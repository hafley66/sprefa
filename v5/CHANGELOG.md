# Changelog

All notable changes to `dl` (sprefa v5) are recorded here. Format follows
[Keep a Changelog](https://keepachangelog.com/); versions track the `v*` release
tags consumed by cargo-dist.

## [Unreleased]

### Added
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

### Guardrails
- SCIP generation is explicit and single-root only. Nothing (daemon, reload gate,
  `scan("*")` fan-out) generates an index automatically; the daemon only imports
  one that already exists. `dl index` refuses an aggregation directory — the XDG
  serving home, or a folder containing nested git repos — unless `--force`, so on
  a machine whose daemon watches hundreds of repos a stray marker file cannot turn
  one command into hundreds of indexer runs.
