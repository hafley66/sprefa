# sprefa

Reactive datalog-over-code engine. Active version is **`v5/`** ("dl"): SQLite-welded,
facts extracted via `scan`+`regex`/`ast`/`sg`/`json`, recursive rules lower to a SQL
fixpoint. `v3/` and `v4/` are prior iterations kept for design-recovery; the OG
coordinate model (strings/refs/byte-spans) lives in `~/projects/sprefa-archive-20260428`.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

## v5 Work — Tasks Context

Active branch `codex/v5-refresh-type-edge`. The recurring debt we keep re-hitting
has two shapes: **(1) per-row write loops (N+1)** and **(2) bespoke per-relation refresh
functions**. A third, **(3) string-inline-everywhere**, is the ref-spine debt.

### Done (this arc)
- [x] Cross-language module graph: all Rust+TS levers ✓ (multi-crate namespace, cross-crate
      `use`, Cargo `package=` rename, `#[path]`, nested braces, raw idents, comment/string
      strip; TS per-package tsconfig, workspace `package.json` fallback, dynamic import).
- [x] rust-analyzer SCIP differential oracle (`tests/oracle_rust.rs`, precision 1.00 on fixture).
- [x] Broken-import linter (`examples/lint-imports.dl`, via `--check`/`--lsp`).
- [x] **Db seam** (`db.rs`): plural-only SQL chokepoint + loud per-tick N+1 counter;
      `refresh_module_rels` migrated to batched `insert_rows`. `conn()` = metered escape hatch
      (36 sites left = grep `.conn()` in engine.rs).
- [x] **B/E/A**: shared `refresh_rel` seam, syn-backed `type_edge(from,to,kind)`, and
      remaining obvious N+1 write loops batched (`_file`, `_prov`, source rows, SCC tables).
- [x] Module-graph polish after B/E/A: `module_edge_rev`/`module_unresolved_rev`
      for historical queries, parallel per-file resolver extraction, and `crate_edge`
      from workspace-internal Cargo dependencies.
- [x] Incremental module refresh for `--changed`: content edits refresh only the
      touched WORK module sources; path-set/manifest changes fall back to the WORK
      rev, and legacy edges rebuild from rev-aware rows so other revs survive.
- [x] SCIP importer tier: existing `index.scip` (or `SPREFA_SCIP_INDEX`) loads into
      `scip_def(symbol,file)`, `scip_ref(file,symbol,def_file)`, and
      `scip_edge(src,dst)` for compiler-backed graph facts.
- [x] Honest RA oracle recall snapshot for real `v5/src`: ignored test reports
      precision 0.86 / recall 0.83 against rust-analyzer SCIP on this checkout.
- [x] Ref-spine C0: v5-native `spine` ID primitives plus `_strings`, `_files`, and
      `_where_bytes` meta tables with zero sentinels.
- [x] Ref-spine C1: source extraction now batches every text value into `_strings`
      with stable `StringId` and normalized text, without changing DSL behavior.
- [x] Ref-spine C1b: WORK file content + committed git blobs now batch into
      `_files`; `FileId` derives from the existing blake3 hash or blob OID
      without per-blob content reads.

### Backlog (sequenced to ADD features without adding dup)

The dup-avoiding order for the `type_edge` feature: **B → E → A** (~M total, leaves *less*
dup than today). Ref-spine **C** stays separate (orthogonal, deferrable).

- [x] **B — generalize built-in refresh** (S, kills dup-shape #2): one `refresh_rel(name, cols, rows)`
      so `refresh_builtin_rels` + `refresh_module_rels` + future `type_edge` share one emit path.
- [x] **E — `type_edge` self-hosted type graph** (S–M, rides B): a `syn`-based extractor (syn
      already in tree) emits `type_edge(from, to, kind∈field|variant|impl|generic)` over `v5/src`
      — same shape as `module_edge`. Then `reaches(Term, X)` = blast radius, fan-in/out per type
      as a query, cycles via `closure(type_edge)`. The deterministic, tokenless type-graph generator.
- [x] **A — migrate remaining N+1 write loops** (S–M, kills dup-shape #1): `refresh_builtin_rels`
      (engine.rs:840-845), `save_file_meta` (765), `retract_paths` (741/749), scc insert →
      `insert_rows`. The ~30 other `.conn()` sites are benign (count-checks, DDL, the fixpoint
      evaluator) — leave or wrap for counting, not N+1.
- [ ] **C — ref-spine Stage 2 remaining** (L, kills dup-shape #3 + unlocks refactor):
      `ref(string_id, file_id, lo, hi)` built-in + file-id-keyed source retraction. C0/C1
      are done: `Coord`/`WhereBytes`/`StringId` math, sentinel `_strings`/`_files`/
      `_where_bytes` tables, batched `_strings` ingestion, and WORK/git-blob
      `_files` ingestion. Next slice should feed captures/source spans into
      `_where_bytes` with INSERT-only batches; then add fuzzy joins/FTS5 trigram
      if needed. Do NOT port FactStore/runtime_graph/Memo/support (DD machinery
      to exorcise).
- [x] **D — module-graph leftover**: incremental `refresh_module_rels` for `--changed`.
      Content edits refresh only touched WORK module sources; path-set/manifest changes
      fall back to the WORK rev. Parallel extraction and rev-aware relation variants are done.
- [ ] **Auto-refactor (the OG v0 use case)**, rides C: thread specifier byte-spans out of the
      module resolver; port `rewrite_use_path`/`reconvert_prefix` from archive `crates/watch/src/rs_path.rs`;
      add an `edit(ref_id, new_string_id)` sink (`--fix` applies, LSP rename). `ref` = import graph
      AND rewrite coordinate; v0's "reverse refs" demo IS the refactor query.
- [x] **SCIP importer (L1')**: ingest an existing `index.scip` from `SPREFA_SCIP_INDEX`
      or repo root into `scip_def`/`scip_ref`/`scip_edge` relations.
- [x] crate-level dep edges (crate A→B from `[dependencies]`) as a relation.
- [x] honest recall: run the RA oracle on a real crate (toy fixture's 1.00 isn't representative).
- [x] **`type_edge` rev-awareness**: `type_edge_rev(from, to, kind, rev)` is the history-aware
      source of truth; legacy `type_edge` is the rev-deduped union (mirrors module_edge split).
      Extractor keeps the rev it already iterated. WORK-vs-HEAD type-graph diff now possible.
- [ ] merge `codex/v5-refresh-type-edge` → main; push. (The earlier `feat/v5-lsp-diag` arc —
      Db seam + architecture doc — already landed on `main` at `f8c8e87`.)

### Style notes for this repo
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
