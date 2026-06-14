# sprefa

Reactive datalog-over-code engine. Active version is **`v5/`** ("dl"): SQLite-welded,
facts extracted via `scan`+`regex`/`ast`/`sg`/`json`, recursive rules lower to a SQL
fixpoint. `v3/` and `v4/` are prior iterations kept for design-recovery; the OG
coordinate model (strings/refs/byte-spans) lives in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`v5/README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

## v5 Work — Tasks Context

Active branch `codex/v5-refresh-type-edge`. The recurring debt we keep re-hitting
has two shapes: **(1) per-row write loops (N+1)** and **(2) bespoke per-relation refresh
functions**. A third, **(3) string-inline-everywhere**, is the ref-spine debt.

### Done (this arc)
- [x] **Multi-repo coordinate** (main, 2026-06-01, commits 83821c3/dfe26ee/60f91ec):
      typed repo/rev on `scan` (Phase 1); `_file`/`_prov` re-keyed on (repo,path,rev)
      so two repos sharing a path don't collide (Phase 2, old dbs migrate on open);
      lazy full-clone of an un-cloned config repo with a `url` (Phase 3); `scan("*")`
      fans one rule over every config repo (the config-folder query). `--root`
      defaults to nearest-.git. **`--move` repo-aware** (7bf9622): `--repo <slug>`
      / `--repo "*"` rewrites a named config repo / fans out over all, each
      processed by `move_one_repo` in isolation (own engine per root). Residual:
      `_where_bytes`/`ref` + module-graph reads still union across repos within a
      single engine (the module-repo-aware follow-on); cross-repo single-pass move
      still routes per-repo to avoid resolver pollution.
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
- [x] Ref-spine C2: regex `match` captures locate into `_where_bytes`, and
      built-in `string(id,text,norm)` + `ref(string,file,lo,hi)` query relations
      project `_strings`/`_where_bytes` via lazy sentinel-skipping
      `refresh_spine_rels`. `ref` is now a reserved name.
- [x] Ref-spine C3/C4: `run_ts`/`run_sg` carry each capture's byte range, so the
      ast and sg backends locate too; `parse_file` keys the located `FileId` off
      the file's stored content address (blake3 for WORK, blob OID for a git rev)
      via `FileId::from_content_address`, so spans join `_files` for both rev
      kinds. Shared `push_span` closure across all three arms.
- [x] Ref-spine C5: `_where_bytes` gains a `path` attribution column (migrated on
      open); `retract_paths` prunes a file's located rows alongside `_prov`, so
      `ref` stays correct across `--changed` edits. (P0 update 2026-05-31: `path`
      IS now folded into the stored id via `WhereBytesId::of_located`, so
      byte-identical files no longer collapse; the old "repaired on full tick"
      invariant is gone.) All ref-spine tests in `tests/spine_meta.rs`.

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
- [x] **C — ref-spine Stage 2** (kills dup-shape #3 + unlocks refactor): C0–C5
      done — `Coord`/`WhereBytes`/`StringId` math, sentinel meta tables, batched
      `_strings` + WORK/git-blob `_files` ingestion, regex+ast+sg located
      `_where_bytes` spans for WORK and git revs, the `string`/`ref` query
      relations, and path-keyed span retraction. Remaining (deferred, "if
      needed"): fuzzy joins / FTS5 trigram; orphan `_strings` GC (interns linger
      after their last `ref` retracts — harmless, content-addressed). Did NOT
      port FactStore/runtime_graph/Memo/support (DD machinery to exorcise).
- [x] **D — module-graph leftover**: incremental `refresh_module_rels` for `--changed`.
      Content edits refresh only touched WORK module sources; path-set/manifest changes
      fall back to the WORK rev. Parallel extraction and rev-aware relation variants are done.
- [ ] **Auto-refactor (the OG v0 use case)**, rides C: thread specifier byte-spans out of the
      module resolver; port `rewrite_use_path`/`reconvert_prefix` from archive `crates/watch/src/rs_path.rs`;
      add an `edit(ref_id, new_string_id)` sink (`--fix` applies, LSP rename). `ref` = import graph
      AND rewrite coordinate; v0's "reverse refs" demo IS the refactor query.
      Plan: `plans/2026-05-31-auto-refactor-use-path-rewrite.md` (3-Sonnet+1-Opus interning
      panel: v4 N+1 was per-row writes + blob-as-string-column, NOT interning concept).
      F1=brace leaves too; F2=Route A (Rust `--move`) now, Route B (DSL operator) deferred
      (never B-naive per-row UDF intern). DONE so far (52/0/1 green, ALL uncommitted):
      **P0** = path folded into `_where_bytes` identity via `WhereBytesId::of_located`
      (byte-identical files no longer collapse → second path lost → retract misfire);
      retires the C5 "collapse repaired on full tick" invariant. **P1** = hoisted
      `insert_spine_where_bytes` out of the per-rule `--changed` loop (latent N+1).
      **F1** = `ModuleRef.span: Option<(u32,u32)>`; `expand_use` threads byte offsets
      (brace leaf span = leaf segment, head shared; bare use = whole path); TS gets the
      specifier-literal span; `module_rows_for_rev` pushes (text, WhereBytes) into
      `ModuleRows.spans`, flushed via new `insert_module_spans` (interns BOTH `_strings`
      AND `_where_bytes`) from full-scan + incremental paths. **ref.id** = `ref` is now
      5-ary `ref(id, string, file, lo, hi)` (id = `_where_bytes` id = the edit coordinate).
      e2e test `use_paths_are_located_in_ref_spine`. NOTE `ref.file` = content FileId, not
      path. Route A LANDED (79db9d9): rspath.rs port + refactor.rs edit sink + `--move OLD=NEW`
      driver + brace head-span (F1b). **Non-src layouts** now rewrite too: `rspath::crate_roots`
      discovers crate roots from the scanned file set (dirs holding lib.rs/main.rs, longest-first),
      `file_to_mod_path_rooted` anchors there (falls back to `src/` off-root), `run_move` threads
      `eng.source_paths()` -> roots into warning+edit+skip loops. Verified on the kernel (no
      Cargo.toml): `--move rust/kernel/clk.rs=rust/kernel/hw/clk.rs` rewrites `crate::clk::Clk ->
      crate::hw::clk::Clk` (was a loud no-op). modgraph resolver left untouched (don't perturb RA
      recall). RESIDUAL: brace-head `use crate::{clk::X, ..}` still left alone (loudly counted);
      physical on-disk file move + moved file's own imports still deferred.
- [x] **SCIP importer (L1')**: ingest an existing `index.scip` from `SPREFA_SCIP_INDEX`
      or repo root into `scip_def`/`scip_ref`/`scip_edge` relations.
- [x] crate-level dep edges (crate A→B from `[dependencies]`) as a relation.
- [x] honest recall: run the RA oracle on a real crate (toy fixture's 1.00 isn't representative).
- [x] **`type_edge` rev-awareness**: `type_edge_rev(from, to, kind, rev)` is the history-aware
      source of truth; legacy `type_edge` is the rev-deduped union (mirrors module_edge split).
      Extractor keeps the rev it already iterated. WORK-vs-HEAD type-graph diff now possible.
- [x] **--move residuals (#17)** (main, 2026-06-12): brace-inner leaves rewrite
      in place via a leaf-level second pass (`modgraph::rust_use_leaves` +
      `rspath::rewrite_brace_leaf`; leaf edits dedup against pass-1 spans, a
      rewrite that leaves the brace head stays a loud skip); the moved file's
      own `super::`/self-references re-anchor (`rewrite_moved_file_use`);
      `--fix` now does the physical rename plus Rust `mod`-decl surgery
      (`refactor::remove_mod_decl`/`add_mod_decl`, parent-chain creation,
      private→`pub(crate)` promotion off the crate root) and rewrites a moved
      .kt file's `package` decl. Child `mod x;` decls of a moved file are
      loudly counted, they do not follow. mod.rs/lib.rs/main.rs moves skip
      surgery loudly. Tests: 3 new e2e in move_refactor.rs + rspath/refactor
      units.
- [x] **Kotlin completion arc** (main, 2026-06-12): Kotlin `type_edge` via
      tree-sitter walk in typegraph.rs (interface keyword split: interface
      supertypes = `generic`, class/object = `impl`, val/var ctor params +
      body properties = `field`, enum entries = `variant`; declared type
      params excluded); expect/actual decl keys map to ALL declaring files
      and imports fan out; same-package implicit edges (`kind="same-package"`,
      word-boundary scan of other files' column-0 decl names); `--move` for
      .kt (ktpath.rs: package math from the moved file's package decl +
      source-root delta; wildcard imports and same-package uses counted
      loudly); scip-java differential oracle (tests/oracle_kotlin.rs +
      tests/fixtures/kt_ws, runtime-skips without JDK/scip-java); git-rev
      content reads batched through one `git cat-file --batch` process per
      repo root (was one `git show` spawn per file); `insert_rows` chunks by
      param budget (32000/ncol rows per statement, was 256) and wraps
      multi-chunk batches in one transaction. Tests: typegraph/modgraph/
      ktpath/db units + kotlin.rs, move_refactor.rs, builtin_file_rel.rs
      (.git-skip) e2e.
- [x] merge `codex/v5-refresh-type-edge` → main; push. Fast-forward (30 commits,
      `f8c8e87..3a8afb4`), full suite green on main, pushed to origin. The arc
      includes type_edge B/E/A, module-graph polish, SCIP importer, and ref-spine
      C0–C5. (The earlier `feat/v5-lsp-diag` arc — Db seam + architecture doc —
      landed earlier at `f8c8e87`.)

- [x] **oxc TS/TSX type_edge producer** (main, 2026-06-12, 1098d80): typegraph.rs
      `ts_edges(content, tsx)` via oxc_parser 0.135 — interface extends/bounds =
      `generic`, class extends/implements = `impl`, properties + ctor parameter
      properties = `field`, enum members + union-alias alternatives = `variant`;
      ref collection rides oxc_ast_visit. engine selects %.ts/%.tsx (.kts
      dispatches before .ts). e2e: tests/type_graph_ts.rs. NOT pushed (default-
      branch push needs Chris).
- [x] **anim-deck.dl emits relational deck rows** (main, 2026-06-12, df9063d):
      node/edge/tag derived from the type graph, node_ref fs refs from decl
      matches (`ref` is reserved — the span spine), tour/tour_step/card/view as
      facts. anim's `atlas-db` fence reads the rel_* tables straight from
      anim/data/sprefa.sqlite.
- [x] **TS function edges** (main, local, cfa48d0): ts_edges treats functions +
      arrow consts as type_edge owners — `param` (input types), `returns` (the
      output type), `uses` (TSTypeReference in the body). The kind vocabulary is
      now field|variant|impl|generic|param|returns|uses.
- [x] **TypeLang common interface + sem entities + SCIP-like resolution** (main,
      local, 030d114): the three extractors sit behind `trait TypeLang { name,
      matches, extract -> TypeFacts }` + `type_langs()` registry (engine asks the
      registry, not the extension; order fixes .kts-before-.ts). New
      `TypeEntity { sym, name, kind, parent, file, line, ty }` is sem's
      SemanticEntity trimmed; `EntityKind` = struct|enum|trait|class|interface|
      alias|function|method|const; a function IS a type via `TypeExpr`
      `[...A] => B`. Three additive rels (type_edge/_rev stay name-keyed):
      **type_entity**(sym,name,kind,parent,file,line) — kills the deck's regex
      decl; **type_sig**(sym,slot,pos,ref) — the arrow exploded, refs resolved;
      **type_link**(src,dst,kind) — the SCIP-resolved sym→sym graph. Resolution
      is hybrid: prefer `scip_ref`'s def_file when an index.scip exists, else
      syntactic name-unique → sym (`scip_name_defs` + `scip_descriptor_name`).
      Reserved-name guard + TYPE_RELS now cover all five. Lines: proc-macro2
      span-locations (Rust), byte-offset index (oxc), node row (tree-sitter).
      Rust emits fn/method entities (self dropped); Kotlin emits type + fn
      entities. Tests: typegraph units, type_graph_ts e2e (entity+sig+resolved
      links), type_entity_xlang e2e (one query finds every "network" interface +
      function across TS and Kotlin). Pushed (Chris ran `git push`, 2026-06-13).

- [x] **typegraph single-parse + Kotlin fn arrows** (main, 2026-06-13, 156f179):
      each `TypeLang::extract` now parses the file ONCE and runs both the entity
      walk and the edge walk over that AST (was a double parse per file). The
      standalone `edges`/`rust_entities`/`ts_entities`/`kotlin_edges` are now
      test-only wrappers over new `*_from` helpers; dead `kotlin_entities`
      removed; `rust_entities`/`ts_entities` are `#[cfg(test)]`. Kotlin `fun`
      entities now carry the arrow `[...A] => B` via `kotlin_fn_type`
      (function_value_parameters -> param slots, trailing type node -> ret;
      declared type-params + builtins excluded), so `type_sig` covers Kotlin
      callables like Rust/TS. Test: `kotlin_function_entities_carry_arrow_types`.
- [x] **mixed source/derived rel now bails** (main, 2026-06-13, ba97aa4): a rel
      headed by BOTH a source rule (scan/match/ast/sg/json/cmd/comment) and a
      derived rule lands in both source_rels and derived_rels; reconcile fills
      the scanned rows, then `rebuild_derived`'s `DELETE FROM rel` drops them.
      `tick` now bails loudly (split into two rels, union in a third) instead of
      silently losing rows. Test: `tests/mixed_source_derived.rs`. anim-self.dl's
      pin/fpin -> span_of split IS the sanctioned shape.

### Open (sprefa type graph)
- [ ] Optional: migrate the deck graph (`examples/anim-self.dl` + anim AtlasPanel)
      from name-keyed `type_edge` to sym-keyed `type_link` + `type_entity` kinds
      for real cross-file edges and function-vs-type node styling. Bigger lift:
      changes node identity (names -> syms), so tour/card/view name references
      and atlas styling must move together.

### Style notes for this repo
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule.
