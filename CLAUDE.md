# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

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
- [x] Honest RA oracle recall snapshot for real `src`: ignored test reports
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
      already in tree) emits `type_edge(from, to, kind∈field|variant|impl|generic)` over `src`
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

- [x] **doc-comment spine (Tier 1/2 doc gen)** (main, local, uncommitted): two
      new builtin rels riding the `TypeFacts` extractor (one parse, populated in
      `refresh_type_rels`): **doc_comment**(repo, sym, line, text) = the cleaned
      doc block per `type_entity` sym; **doc_tag**(repo, sym, tag, arg, text) =
      structured split. Per-language AST locators in typegraph.rs: Rust =
      syn `#[doc]` attrs (so `#[derive]` between doc and decl is a non-issue),
      Kotlin = tree-sitter preceding KDoc sibling, TS = oxc byte-association
      (`/** */` → nearest anchor with whitespace-only gap; top-level + class
      methods + var-fns, decorated classes skipped). Tags: shared `parse_jsdoc_tags`
      (`@param`/`@returns`/`@deprecated`, `{type}` dropped, name→arg) for JSDoc/KDoc;
      `parse_rust_sections` (`# Panics`/`# Examples` → section tags) for rustdoc.
      `DOC_TEXT_RELS` gate + reserved-name guard; `doc_text_rels_used` triggers
      `refresh_type_rels` so a doc-only program works. Tests: doc_comment.rs e2e
      (all 3 langs, both tiers). Example: `examples/doc-coverage.dl` (undocumented-
      API rail via `!doc_comment`). NOT a CI gate (docs). Survey of cross-lang doc
      conventions in chat_log; Python/Go need their own TypeLang first (SCIP-only
      today). See [[project_doc_comment_spine]].

- [x] **interprocedural value flow (typed, SCIP-resolved)** (main, local,
      uncommitted): `examples/flow-interproc.dl` + `tests/it/flow_interproc.rs`.
      Unions the intra-proc `df_edge` lift with interprocedural hops into one
      `flow_edge`; `closure(flow_edge)` walks value flow ACROSS fns — the join the
      df_* lift and the scip_*/call_* graph couldn't make alone. FORWARD hop: an
      arg feeding a `call_res` flows into the resolved callee's params. BACKWARD
      hop: new `ret` df_node kind (Rust `Expr::Return` + block tail; Kotlin
      jump_expression + body tail; TS ReturnStatement + arrow expr-body) lets a
      value passed in, transformed, returned reach the caller's call_res end to
      end. Resolution rides `call_edge` (SCIP-preferred already; dodges the
      df_node line-base gotcha — Rust 1-based vs Kotlin/TS 0-based). TS arrow /
      fn-expression consts now lifted as their own fn scope (`ts_lift_fn`). New
      builtin rel `df_param(id, pos)` (typed-param index, self-skipped to align
      with `type_sig.pos`) powers the node-level `flow_node_type` view. Limits
      (honest, in the .dl header): positional-blind forward hop,
      context-insensitive. Line-base normalization deliberately skipped (would
      desync loop_over/nest spans). Suite green: it 306/0/3, lib 162/0/1. See
      [[project_interproc_flow]].

- [x] **Ports + MCP epic** (main, 2026-07-01, fc1c977..4ac9cff, pushed):
      lattice decl qualifiers `key(...)`/`merge(MaxBy(col))` (Soufflé
      choice-domain + row-selection lattice); `@in(class)`/`@out(class)` port
      qualifiers on the same seam (class = contract, rpc only; envelope
      checked by column name at declare; rules/facts heading an @in port bail
      — the serving loop is the only writer; ambient recv/send globals
      REJECTED and torn out); `--mcp` = the rpc->stdio×jsonrpc binding
      profile (transport NEVER in .dl). Envelope id = raw JSON text of the
      request id (int + string ids round-trip); notifications silent;
      unanswered id -> -32601; drain law 1 retires answered rows. Daemon-first
      pump mirrors --hook: `mcp_request`/`mcp_retire` RPCs beside query_sql,
      port-validated against the DAEMON's program (drift guard);
      mcp.rs Pump{Local,Daemon}; --no-daemon = hermetic CI path.
      examples/mcp-echo.dl (lattice dispatch) + examples/mcp-server.dl
      (registerable: initialize/tools/list/tools/call as rules, tools/call
      params via term-form jsonp). Harnesses: tests/it/mcp.rs (8),
      mcp_lifecycle.rs (4, drives the real example through the client
      handshake), mcp_daemon.rs (2, tick-counter proof + non-port refusal).
      RESIDUAL (decided, unbuilt): bind("rpc","stdio","jsonrpc") facts
      cascade (CLI > bind facts > class default); setup --project writing
      .mcp.json; adapter built-in dl.query/eval tool bridging the daemon's
      eval RPC; law-2 digest sent-log -> @yield -> stream class.

- [x] **Want-tier demand built-ins** (main, 2026-07-01, 8111515): `git_ref`
      (ref inventory across self+config repos, annotated tags peeled) +
      `rev_behind(repo, refname, upstream, behind, ahead)` filled from a
      user-DERIVED `rev_cmp_want` (convention read, the org-allowlist
      pattern; unresolvable refs AND shallow clones skip loudly — corpus
      depth-1 clones produced 2111 false diverged before the guard) +
      `scip_want(repo)` lazy multi-repo SCIP (ensure_index per wanted root
      runs installed indexers only when the index is missing, merge_files,
      ONE load so cross-repo refs resolve; no schema change). Demand hops
      compose at one tick each (pin-skew chain = 3 ticks). `repo_want`
      unneeded: repo-sink rules already register dynamically. `rel_rows`
      now stringifies non-text columns (int reads silently dropped rows).
      examples/pin-skew.dl = the proving query (go.mod seam -> pin ->
      stale_pin/diverged_pin; bespoke lockfiles union into pin); corpus
      smoke: 120 stale pins on the one unshallowed hub (go-retryablehttp,
      cross-org rows), 218 shallow skips, 0 false diverged. Tests:
      tests/it/git_ref.rs, scip_want.rs, pin_skew.rs.

- [x] **Perf-under-reactivity arc** (main, 2026-07-02): profiling
      flow-interproc on this repo found four walls; three fixed + telemetry.
      (1) `rebuild_derived` one-pass for non-recursive components:
      `rel_components` splits each stratum by rel-level Tarjan (ascending
      comp id = dependencies first; self-edges tracked separately), only
      recursive components iterate — the old loop re-ran EVERY statement to
      observe delta=0, a structural 2x on expensive rules (40s statement ran
      twice). (2) Closure-query guard: unpinned/`?`-view closure queries
      refused over `DL_CLOSURE_QUERY_MAX_EDGES` (20k default) — LIMIT does
      NOT short-circuit the view (UNION + recursive CTE materialize first;
      measured >10s for LIMIT 5 on 471k edges); both-pinned queries answer
      via `run_reaches_pair` condensation walk; `closure-unpinned` warning in
      dl_diag is the lint twin. (3) call_edge_bare equality bridge in
      flow-interproc.dl + taint.dl kills the per-pair replace() suffix test
      (~25M evals -> 2.5k row-local strips); cold derived 130s+ -> 1.4s.
      (4) NEW `rel_count`/`stmt_ms` telemetry built-ins (src/rels/perf.rs;
      `_stmt_ms` meta table written batched by rebuild_derived; closure VIEWS
      excluded from counts — counting one materializes the closure, found by
      dogfood hang) + examples/perf-rails.dl budget rails. BONUS: multi-file
      one-shot merge fixed (`dl a.dl b.dl` silently dropped all but the first
      positional in every one-shot mode; now merges, in-process, daemon gate
      len<=1). MEASURED warm no-change tick: 1.56s, of which 515ms = the
      type/call/dataflow full-corpus re-parse — the remaining known wall
      (gap A; per-file fact cache keyed on content hash is the fix shape,
      template refresh_module_rels_for_paths). Gaps B (full tick lacks
      affected_derived scoping) and C (families dirty-mark unconditionally)
      documented in chat_log/20260702.0. Tests: closure_query_guard.rs,
      perf_rels.rs, dl_diag closure lint, rel_components units.

- [x] **Perf gaps A/B/C + RelDecl doc consolidation** (main, 2026-07-02,
      local): **A** = type/call/dataflow/doc refreshers persist an
      `extract:<family>` input digest (corpus (repo,path,rev,hash) rows +
      scip_ref + exe identity, `_reldigest` keys) and SKIP the whole
      parse/resolve/write pass warm; per-file fact cache (repo,path,hash) ->
      Arc<facts> re-parses only moved files on a changed tick. Measured:
      warm flow-interproc tick 1.5s -> ~35ms in-engine (extraction
      183/281/930ms -> 0.3ms each). **B** = the FULL tick now attributes
      changes per rel (seed_rel_digests returns movers; family/RelKind
      refresh bools; NEW `async:<rel>` content digest for @async/@stream
      response rels — the drain writes off-tick, nothing else attributes
      them, gh-cache latest-wins broke without it) and scopes
      rebuild_derived + closures + post-stratum via affected_derived, same
      walk as tick_paths; blank slate / program edit / @next carry still
      full. **C** = tick_paths marks a family's rels changed only when its
      digest moved (editing .md no longer re-derives a type program's deps).
      Instrumentation: `extract_files_parsed`, `last_derived_rebuilt`;
      tests extract_cache.rs (3) + scoped_tick.rs. **Consolidation** =
      builtin_rel_docs() tuple registry DELETED; RelDecl carries
      group/doc (&'static str, "" for user decls), all 65 decl sites
      annotated, catalog + undocumented_builtins read decls, README regen
      byte-identical. **dl setup --project** wires `assets/*.skill.md` ->
      `.claude/skills/<name>/SKILL.md` relative symlinks (the three
      maintainer checklists on a fresh clone). Suites lib 191/0/1, it 434/0/4.

- [x] **Positional + constructor dataflow** (main, 2026-07-02, local):
      `df_arg(call, pos, arg)` (0-based slot per argument, method receiver -1,
      aligned with df_param.pos/type_sig.pos) + `df_field(id, field, value)`
      (Rust struct-literal fields, TS object-literal properties, Kotlin named
      args; ".." = spread/FRU base) across all three TypeLang lifts.
      Instantiations = `new` df_nodes w/ the type name (Rust struct literal +
      capitalized tuple-struct/variant ctor, TS `new`/object literal, Kotlin
      capitalized ctor call); field reads = `member` nodes w/ the accessed
      name (Rust Expr::Field + Kotlin navigation were `expr` catch-alls with
      NO base edge — flow hole closed); receivers flow into method results in
      all 3 langs. flow-interproc.dl + taint.dl forward hop now POSITIONAL
      (df_arg.pos = df_param.pos, arg 0 no longer reaches param 1);
      flow-ctor.dl = ctor inventory + field_fill + field-SENSITIVE field_flow
      (new-seeded recursive rule, NOT closure — a closure rel can't be read
      unpinned in a rule body). nest counts `new` too (ctor in a loop
      allocates per iteration). Tests: 3 typegraph units, flow_ctor.rs e2e,
      position gate in flow_interproc.rs. DATAFLOW_RELS 6->8.

- [x] **JSX dataflow** (main, 2026-07-02, local): JSX element = `new` df_node
      w/ component/tag name + df_field per attribute (bare bool = lit, spread
      "..", children under "children" — the jsx(Comp, props) desugar made
      facts); component usage = call SITE (IdentifierReference/Member only,
      host elements skipped) so call_edge resolves caller->Card and
      call_name(sym, "Card") is the indexable name handle; TS destructured
      object params mint one param df_node PER property (var = KEY name for
      the name-match, scope binds LOCAL name, shared slot index; was a total
      hole — ts_binding_name returned None so React components had NO param
      nodes). ts_seed_params replaces the 3 dup param loops.
      examples/flow-jsx.dl = jsx_use + prop_edge (name-matched via call_name
      equality, param + member targets). Tests: tsx/destructure units,
      flow_jsx.rs e2e (undeclared-prop negative). Suites lib 196/0/1,
      it 439/0/4.

- [x] **Dataflow roadmap 1-6 (std/flow.dl arc)** (main, 2026-07-02, local):
      all six ranked items in one arc. **(3)** std/flow.dl = the shared base
      as a `use` module (call_edge_bare, flow_edge union, call_node);
      flow-interproc/taint/flow-jsx rebased on it (flow-ctor shares nothing,
      untouched). **(1)** flow_summary(callee,pos)/flow_sanitizer(callee) =
      propagation MODELS: the lift's blanket arg->result edge is CUT for
      modeled callees except summarized slots (additive summary would be
      redundant — the blanket edge already exists; suppression is the real
      semantics, sanitizer = zero-slot instance); stratified via
      flow_kept/flow_cut, free when no facts. **(4)** arg_field_flow =
      prop_edge generalized to plain calls (df_field on an arg composite ->
      same-named member/param reads in the resolved callee). **(2)** inline
      lambdas lift as own fn scopes across 3 langs (Rust closures, TS inline
      arrows/fn-exprs, Kotlin lambda literals incl. trailing-lambda syntax
      which wasn't even an ARG before; implicit `it` = param 0; `closure`
      value node carries the lifted sym in var = the join key; captures via
      shared scope in Rust/Kotlin; nest loop-matching ::closure::-prefix
      aware) + flow_lambda/flow_lambda_ret fact-driven hops;
      std/flow-collections.dl ships map/filter/fold/... facts. **(5)**
      examples/flow-services.dl = the wire hop (spec-seeded service_op from
      openapi.yaml operationId — NB `**/openapi.yaml` glob, brace-set
      `{yaml,yml}` doesn't match; stub+handler SHARE the name so single-def
      resolution refuses exactly where the wire hop takes over). **(6)**
      per-call-site context: Kotlin df lines normalized to 1-based (+1 rows
      AND loop spans, ids keep raw rows; TS was already 1-based via line_at —
      old "Kotlin/TS 0-based" note was half wrong), Rust method-call node
      moved to the METHOD ident line (multiline chains join call_site now),
      call_node = ONE equality join, and NEW call_target(call,caller,callee,
      callee_q) pins both interproc hops per call site — f(secret);g(benign)
      no longer cross-talks args OR returns. call_target factored as its own
      rel ALSO for the planner: the inlined 7-atom fwd hop = ~7s/tick on this
      repo (stmt_ms found it), factored = ~0.5s whole graph; cold derived
      1.07s (pre-arc 1.4s baseline) w/ strictly more precision. Dogfood:
      taint.dl on this repo 161 -> 9 findings (the deleted rows were
      per-caller cross-talk). Residual (documented, deferred): callee
      param->ret path still merges callers (k-CFA cloning out of scope);
      flow_lambda facts are name-keyed so ecosystem-ambiguous by design.
      Tests: 3 typegraph units, flow_std.rs (6, incl. per-lang cross-talk +
      fact-driven collection gates), flow_services.rs (2, incl. no-spec
      negative). Suites lib 199/0/1, it 447/0/4.

### Open (sprefa type graph)
- [x] **BUG FIXED: lattice hot-reload wedge** (found 2026-07-03 live,
      fixed 2026-07-04, local uncommitted): `Engine::declare`
      (engine/mod.rs:2833) migrated a cached table only on COLUMN-set
      drift; a key(...)/merge(...) change with identical columns kept the
      old full-row PK under CREATE IF NOT EXISTS, so the lattice upsert's
      ON CONFLICT(key) had no matching constraint and every tick wedged.
      Fix: the same migration block now also reads the existing PK
      (PRAGMA table_info) and drop+recreates on PK-set drift (order-free
      compare; _reldigest row deleted so the rel re-derives). Matrix:
      add/remove/change-key-cols = drop+recreate; merge-fn-only = no
      drop (upsert SQL regenerates per tick). Wedge reproduced before
      fix (env-gated), gone after. Tests tests/it/lattice.rs (4, warm-db
      reload path). Suites 204/466. NOTE: the RUNNING daemon still has
      the old binary — wedge remains live-possible until the binary
      upgrade (same morning action as df_node_repo).
- [x] **BUG FIXED: "corpus-flat" name resolver** (found 2026-07-03 D1,
      fixed same day, D5a, local uncommitted): root cause was NOT flat
      keying — `by_name` in `refresh_type_rels`/`refresh_call_rels`
      (src/engine/extract.rs) is correctly keyed (repo, name), but pushed
      one entry per raw fact occurrence. When a root is registered twice
      under one rid (bench diff.config.toml slug `head` + self slug both →
      basename rid `vscode-flow-panel`), every def sym landed twice →
      len 2 → read as ambiguous → that whole repo resolved bare while the
      single-scanned repo resolved fine (looked one-sided: 142/0
      type_link.dst, 2612/0 call_edge). Fix: dedup the ambiguity bucket by
      def sym before counting. Identity gate now exact 0 across all
      resolved kinds; test tests/it/resolver_repo_scope.rs (two fixture
      repos w/ colliding names, one double-registered). Suites 202/459.
- [x] **BUG FIXED: SCIP importer cross-root collapse** (found 2026-07-03
      D3, fixed same day, local uncommitted): per-index load with origin
      repo threaded (`rows(index, root, slug)` + `repo_of()` nearest-.git
      basename matching engine `repo_id_of`; `index_inputs` replaces the
      merged-path `resolve_index`; `merge_files` kept only for the
      `dl index` artifact); scip_def/scip_ref/scip_edge gained trailing
      `repo` col (dl arity is exact — 13 in-tree positional readers swept
      with `_`); `scip_name_defs` keyed (repo, file, name), both resolve
      closures repo-scoped, cross-repo SCIP resolution dropped;
      `extract_input_digest` folds scip_ref.repo (byte-identical triples
      XOR-cancelled → false family-skip). Gate: second index now ADDS
      3548/4724 (was zero net, total 7096/9448 split evenly); SCIP-on
      diff == SCIP-off diff byte-identical. Suites 203/459. KNOWN LAG:
      scip_want consumption lands tick 3 (want t1, ScipKind load t2,
      extract reads prior-tick scip_ref t3) — pre-existing ordering, not
      new.
- [x] **BUG FIXED: df family read config repos at self-root** (found
      2026-07-03 D4 S2, fixed 2026-07-04, local uncommitted):
      `refresh_dataflow_rels` (extract.rs:1056) read every file via
      `read_content(&self.root, ...)` — config-repo paths resolved under
      the WRONG tree (usually missing → zero df rows). type/call already
      used `roots.get(repo)`; df now matches. Second defect fixed en
      route: df rows are path-keyed (no repo), so flow-panel.dl's
      name-joined field/fill rules fanned one repo's fill across all
      repos — NEW builtin `df_node_repo(id, repo)` (DATAFLOW_RELS 9,
      emitted per (id,repo) occurrence, NOT first-seen-deduped) +
      repo-scoped joins in flow-panel.dl. Digest was already
      WORK-hashed (fix restored read/digest symmetry). harness.sh S2
      re-armed, exits 0 on a synced pair (all 4 scenarios exact). Tests:
      dataflow.rs config-repo WORK e2e. Suites 204/460.
- [ ] **SERVED-COPY DIVERGENCE (morning action)**: the live daemon runs
      installed 0.4.1 which lacks `df_node_repo`, so the served
      ~/projects/sprefa/.dl/flow-panel.dl is a COMPAT DOWNGRADE (the six
      `df_node_repo(n, repo),` atoms stripped; helper rels kept). The
      worktree .dl/flow-panel.dl is the real version. After upgrading
      the daemon binary (cargo install --path . or restart on
      target/release/dl — restart was auto-denied overnight), resync:
      cp <worktree>/.dl/flow-panel.dl ~/projects/sprefa/.dl/. Also note
      harness vs real sprefa-base pair reads nonzero baseline until the
      day's arc is committed and sprefa-base fast-forwarded (the
      scratch-pair run is the exit-0 proof).
- [x] **FIXED: `type_entity.parent` real-kind owner keys** (2026-07-03,
      local uncommitted): Rust minted every method parent as
      `EntityKind::Class` (typegraph.rs) — now a per-file name→kind first
      pass (`rust_owner_kinds`) mints the owner's real kind, so
      parent joins type_entity.sym with zero normalization (same-file
      mismatches 928→0). TS `push_entity` takes `(name, kind)` for parent
      (methods are class-owned, correct by construction); Kotlin members
      were flat (parent None), untouched. `owner_classkey` workaround
      DELETED from .dl/flow-panel.dl (member rules join raw_parent
      against owner_entity.sym directly); served copy synced, no daemon
      wedge. Join-property test `entity_parent_joins_owner_sym_across_langs`.
      Suites 204/459.
- [x] **FIXED: cross-file `impl` parents resolve to the declaring file**
      (2026-07-04, local uncommitted): `refresh_type_rels`'s qparent
      computation resolves a dangling parent's owner NAME through the
      existing D5a bucket machinery (`resolve(repo, file, name)`, SCIP
      preferred when indexed) — unique-in-repo rewrites to the declaring
      file's exact sym, ambiguous stays file-scoped (dangling is honest).
      Same-file parents untouched. Unmatched 66/1020 → 0/1020 on this
      checkout. Tests tests/it/entity_parent_xfile.rs (resolve +
      ambiguous-stays). Suites 204/462.
- [ ] Small extractor gaps (flagged in the join-property test): Rust
      trait default methods and Kotlin `object` decls emit no
      type_entity rows.
- [ ] Optional: migrate the deck graph (`examples/anim-self.dl` + anim AtlasPanel)
      from name-keyed `type_edge` to sym-keyed `type_link` + `type_entity` kinds
      for real cross-file edges and function-vs-type node styling. Bigger lift:
      changes node identity (names -> syms), so tour/card/view name references
      and atlas styling must move together.

### Style notes for this repo
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.
