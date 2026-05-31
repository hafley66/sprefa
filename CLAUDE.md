# sprefa

Reactive datalog-over-code engine. Active version is **`v5/`** ("dl"): SQLite-welded,
facts extracted via `scan`+`regex`/`ast`/`sg`/`json`, recursive rules lower to a SQL
fixpoint. `v3/` and `v4/` are prior iterations kept for design-recovery; the OG
coordinate model (strings/refs/byte-spans) lives in `~/projects/sprefa-archive-20260428`.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

## v5 Work — Tasks Context

Branch `feat/v5-lsp-diag` (unmerged, unpushed). The recurring debt we keep re-hitting
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
- [ ] **C — ref-spine Stage 2** (L, kills dup-shape #3 + unlocks refactor): `_strings` interner
      + `ref(string_id, file_id, lo, hi)` built-in + content-derived ids (`StringId=blake3(value)`,
      `FileId=blake3(content)`). Port `Coord`/`WhereBytes`/`StringId` math from `v4/src/lib.rs`
      + `_strings`/`_where_bytes` from `v4/src/store.rs`; `normalize()` + FTS5 trigram from the
      archive for cross-repo fuzzy join. Content-ids make the interner INSERT-only (no read-back
      N+1). Do NOT port FactStore/runtime_graph/Memo/support (DD machinery to exorcise).
- [ ] **D — module-graph leftover**: incremental + parallel `refresh_module_rels` (today wholesale
      re-read + re-resolve every tick); rev-as-variable soundness (`refresh_builtin_rels` hardcodes
      `"WORK"` engine.rs:837 → git-rev/historical graphs silently empty).
- [ ] **Auto-refactor (the OG v0 use case)**, rides C: thread specifier byte-spans out of the
      module resolver; port `rewrite_use_path`/`reconvert_prefix` from archive `crates/watch/src/rs_path.rs`;
      add an `edit(ref_id, new_string_id)` sink (`--fix` applies, LSP rename). `ref` = import graph
      AND rewrite coordinate; v0's "reverse refs" demo IS the refactor query.
- [ ] crate-level dep edges (crate A→B from `[dependencies]`) as a relation.
- [ ] honest recall: run the RA oracle on a real crate (toy fixture's 1.00 isn't representative).
- [ ] merge `feat/v5-lsp-diag` → main; push.

### Style notes for this repo
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
