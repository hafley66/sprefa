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

- [x] **D5 rev-aware extraction arc** (worktree, 2026-07-04, local): per-rev
      `extract:<family>:<rev>` digests + `refresh_rel_for_revs`; twins
      `type_entity_rev`/`type_link_rev`/`call_def_rev` (rev = column, sym
      stable cross-rev) + df twins `df_node_rev`/`df_node_repo_rev`/
      `df_arg_rev`/`df_field_rev` (ids rev-salted `{rev}\u{1}{raw}`, legacy
      keeps raw ids via dual-emit); per-(repo,rev) resolution, SCIP consulted
      at rev=="WORK" only; `sweep_gone_revs` retracts a vanished rev from all
      11 twins + digests + legacy unions same tick (module twins included,
      previously lingered). Consumers: .dl/graph-diff.dl = diff_pair(rev,rev)
      anti-joins on ONE checkout (shipped default = inert (WORK,WORK) — a
      committed base scan would union into the legacy rels for every daily
      consumer); examples/pr-diff.dl = PR diff via gh effect -> shas -> scan
      rev slots, self-contained pr_* rels (diff_pair is a fact; a derived twin
      would mixed-kind bail); harness.sh = 4 scenarios single-checkout,
      worktree pair + sprefa-base + basename prefix convention RETIRED. Panel
      contract survived zero HTML logic changes. Plan:
      plans/2026-07-04-d5-rev-aware-extraction.md.
- [x] **hook_event seam + chat-marks** (2026-07-04, local): generic builtin
      `hook_event(kind, session, seq, json)` fed by `dl --hook` (daemon RPC in
      the mcp_request idiom + in-process fallback); setup registers dl under
      UserPromptSubmit AND PostToolUse; output arm echoes the received event
      name. examples/chat-marks.dl = `@@mark <title>` sections every following
      message via per-message negation argmax; phrase lives in the .dl ONLY
      (wasm-generality law: engine ships seams, policy lives in programs).
- [x] **CLI discovery + learning surfaces** (2026-07-04, local): --help gains
      help_heading groups + SUBCOMMANDS/LEARN MORE/AUTHORING trailers;
      `dl docs` embeds reference + book (ch0-8 incl. new 08-argmax) + NEW
      hands-on tutorial (book/tutorial/, 9 lessons, outputs captured from real
      runs) + `authoring` topic (= the skill). Doc indexes DOGFOODED:
      examples/gen-doc-indexes.dl generates book/README + tutorial/README
      lists AND docs_cmd.rs include_str/table rows from a scan of book files
      (blurb = each file's first `>` blockquote), drift rail exits 2 on
      file-added-regen-not-run.
- [x] **Agent sharp-edges arc** (2026-07-04, local, spec docs/agent-sharp-edges.md):
      skill survival block + per-op language matrix (CI-honest via
      tests/it/lang_matrix.rs against SG_LANG_TABLE/AST_LANG_TABLE);
      `--parse-only` no-scan validate (parse+typecheck+metavar sanity+ALL
      regex literals compiled via shared `compile_dl_regex` — lookahead
      fast-fails sub-second); `lowercase-metavar` warn lint; head-var-not-bound
      + unbound-constraint + regex errors now name the fix. NOT built:
      pre-scan binding analysis (residual 2, needs scope analysis).
- [x] **Docs/examples polish** (2026-07-04, local): op_docs()/README/reference
      regenerated with descriptive vars (NEW REPO LAW: no single-letter dl
      vars anywhere — skills/examples/book/tests/prompts); 4 skill assets
      refreshed (5 stale arcs; install path was pre-lift `--path v5`, the
      new-extraction-op example was fictional — replaced with real ast_yaml);
      examples/endpoint-flows.dl (axum routes -> call-graph reach -> info
      diags = hover shows "in endpoint flows: GET /users"); deck/ = presenterm
      slideshow w/ dl.sublime-syntax via bat cache (presenterm syntaxes are
      compile-time baked; bat+exec_replace is the sanctioned route).

- [x] **sg grammar sweep + term-form sg** (worktree, 2026-07-04, b5b61a8):
      SG_LANG_TABLE 10 -> 23 grammars (css, html, bash, csharp, java, scala,
      swift, ruby, php, lua, elixir, haskell, yaml); term-form
      `sg(:lang, bound_str, "pattern"[, spans])` — leading :lang dispatches,
      Sg src:Term + rev:Option<Term> rides the eval_extract_rules seam,
      spans REGION-relative; examples/styled-components.dl + md-fences.dl
      (embedded-lang seam: match feeds line-wise, "..." literal drops \n
      backslash so backticks). lang_matrix.rs keeps the skill honest at 23.
      Ledgered follow-up: absolute-span composition for rewrite-grade
      embedded matches.
- [x] **comment_node + std/suppress** (worktree, 2026-07-04, 9d0a97c):
      builtin `comment_node(path, line, col, end_line, end_col, text, kind)`
      = generic tree-sitter walk (kind().contains("comment")) + oxc
      program.comments for TS/TSX; string-literal-safe by grammar; own
      CommentFamily gate/cache/digest; kind = line|block|doc (classify by
      marker). std/suppress.dl = the eslint/biome disable grammar entirely
      in dl: dl-disable-line (INLINE works — sharp-edge #5 resolved)/
      next-line/block pairs via argmax pairing + EOF sentinel, code scoping,
      `-- reason`, wildcard "*" rows, lint_candidate/rail_finding
      conventions, self-hosting disable. lint-unwrap.dl converted. Tests:
      comment_node.rs + suppress.rs (10).
- [x] **directive visibility** (worktree, 2026-07-04, ce79682): dl's own
      magic shows itself — info diags (code `dl-directive`) at directive
      comment col spans, `dl-directive-malformed` warn (typo'd markers no
      longer fail silent), `dl-suppress-unused` warn (a disable matching
      nothing). All in std/suppress.dl; severity mapping note: hint renders
      invisibly in vscode, info = subtlest reliable dot.
- [x] **diag_mute seam** (worktree, 2026-07-04, 21b68cf): writable
      `diag_mute(code)` builtin (hook_event precedent: always declared,
      written out-of-tick, never a rule head) + Engine
      toggle_diag_mute/muted_codes/diag_code_states; LSP executeCommand
      `dl.toggleDiagCode`/`dl.listDiagCodes`; filter at the PUBLISH seam
      only — --check/--parse-only read `diag` directly, unaffected (editor
      affordance, not a CI gate). vscode 0.4.2: quick-pick toggle +
      cmd+alt+d cmd+alt+d chord (cmd+alt+m was taken by markSelection).
      README/relations regen swept in decl drift from the prior arcs.
      Tests tests/it/diag_mute.rs (4, incl. --check-unchanged e2e + full
      LSP round-trip). Suites 209 lib / 515 it.

- [x] **File-watcher scaling + deep-root daemon** (2026-07-06, local
      uncommitted): the daemon watcher woke on every write under a served root
      (no ignore filter, whole `.git` recursive) — `target/`/`node_modules/`/
      `.git/objects` churn the engine never scans, each event taking the drain +
      engine lock + git-family refresh + idle-timer reset. NEW `src/watchgate.rs`
      `WatchGate`: a receive-side gate mirroring the scan corpus's include
      decision (per-root `GitignoreBuilder` incl. `.git/info/exclude` + global,
      `matched_path_or_any_parents` so a file under `target/` drops; `.git`
      pruned to the narrow `HEAD`/`packed-refs`/`refs/` ref paths; daemon
      bookkeeping dropped — `is_daemon_internal` moved here). Both prefix forms
      (as-given + canonical) stored per root/git-dir since notify paths are
      canonical on macOS (FSEvents) but as-watched on Linux. **P0** =
      `socket_path` relocates the socket to `/tmp/dl-sock/<blake3-16hex>.sock`
      when `<root>/.dl/daemon.sock` ≥ 100 bytes (macOS `sun_path` cap); single
      chokepoint so bind + every connect agree; pid/log/cache stay root-local.
      **P2** = fixed 150ms drain → `recv_timeout` quiet-period debounce
      (QUIET 120ms, MAX_WINDOW 600ms cap) in both `watcher_loop` and the
      pre-daemon `run_watch` twin. **P3** = narrow git watch (git dir
      NonRecursive + `refs/` Recursive, NOT `objects/`); applied to
      dynamically-pulled repos too (closed the deferred gap). **P4** =
      `need_rescan()`/`notify::Error` → loud `tick_full` recovery (was silently
      dropped). **P5** = `d.touch()` gated behind the filter, so pure noise no
      longer resets the idle timer. Linux per-dir inotify pruning deferred
      (FSEvents is one stream/root; sketch in plan). Tests: watchgate units (6),
      3 daemon e2e (deep_root short socket + bind, gitignored-writes-no-tick,
      source-burst-coalesces). SCALE: `event_volume_gitignored_subtree_scale`
      (macOS-runnable) writes 2000 gitignored files (~108ms) → 0 ticks + a
      needle source edit STILL ticks (gate scales as a filter, not a blanket
      drop); `linux_inotify_watch_count_tracks_unignored` (#[cfg(linux)] +
      #[ignore]) reads /proc/<pid>/fdinfo and asserts watch count is
      O(unignored dirs) — FAILS today by design, it IS the spec for the deferred
      per-dir pruning (drop #[ignore] when that lands). Suites 215 lib / 520 it
      (+1 scale, +1 ignored linux). Binary reinstalled; live daemon restart
      still Chris-only (served-copy divergence item).

- [x] **Magic-rel ban — ELIMINATED** (2026-07-06, local uncommitted): the
      invisible-API pattern (engine reads a rel by LITERAL string name to trigger
      IO — `rels.get("scip_want")`, `FROM rel_effect_cmd`) is RETIRED, not just
      documented. The four demand/overlay conventions (scip_want / rev_cmp_want /
      def_target / effect_cmd) are now first-class **builtin SINKS** like
      diag/repo: pre-declared in `demand_rel_decls()` + reserved via
      `DEMAND_RELS` (src/engine/mod.rs, mirrors diag_rel_decls/DIAG_RELS),
      catalogued group `demand`, head-written from a rule (no `rel` decl — the
      guard bails "head it directly, like diag/repo"). Consumers migrated: 10
      `rel <name>` decls dropped across 4 examples/.dl + 6 test programs. The
      first-pass `special_rel` REGISTRY was DELETED (src/rels/special.rs gone,
      SpecialKind unwired, read-site consts reverted to literals — clean now that
      the names are catalogued). Precedent: `repo` (catalogued group `core`,
      head-written, triggers cloning). Rail `.dl/magic-rel-audit.dl` scans
      `src/**/*.rs`, anti-joins `rel_catalog` ONLY (one known-set),
      `magic-rel-unregistered` --check exit 2 (CI bare-check + PostToolUse hook).
      Agent-side: `assets/sprefa-v5-no-magic-rels.skill.md` (wired by `dl setup
      --project`) + `.claude/agents/magic-rel-auditor.md` subagent — adding a
      demand convention = a RelDecl in demand_rel_decls() + DEMAND_RELS, never a
      hidden name. Docs `docs/reference/magic-rels.md` generated from rel_catalog
      group demand by gen-reference.dl. Test `tests/it/magic_rel_audit.rs` (3)
      guards the rail from rotting. NO `@`-qualifier, no new syntax — Chris
      rejected an at-symbol binding; the answer was "they were always just
      builtin sinks we forgot to declare." Suites lib 219 / it 530 green; binary
      reinstalled (installed dl needs the `demand` catalog group + pre-declared
      heads). Plan: plans/2026-07-06-magic-rel-audit.md.

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
- [ ] **SERVED-COPY DIVERGENCE (remaining: daemon restart, Chris only)**:
      ~/.cargo/bin/dl IS current (cargo install ran 2026-07-04 through the
      parse-only-regex arc), but the RUNNING daemon (pid in
      ~/projects/sprefa/.dl/daemon.pid, started Jul 3) still executes the
      old image — restart was auto-denied to the agent twice (live-infra
      boundary). Restart: kill $(head -1 ~/projects/sprefa/.dl/daemon.pid),
      then nohup ~/.cargo/bin/dl --daemon --root ~/projects/sprefa. After
      restart: cp <worktree>/.dl/flow-panel.dl ~/projects/sprefa/.dl/
      (replaces the df_node_repo-stripped compat downgrade) and
      `dl setup --project` in served repos to register the UserPromptSubmit
      hook (activates chat-marks `@@mark`). The old sprefa-base worktree-pair
      note is RETIRED: D5.7-9 moved the diff to rev pairs on one checkout,
      bench/graph_diff/harness.sh no longer references sprefa-base.
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

### Open (demand-sink + daemon ergonomics — 2026-07-08 agent-session feedback)
Five complaints from other AI sessions after the `checkout` sink shipped (v0.6.18).
ALL ADDRESSED 2026-07-08 (v0.6.19); #2 was a misread (no code change). Kept as the
record; see CHANGELOG 0.6.19. Fixes: #3 example url-gate dropped + warning; #1 repo
sink takes ground facts (explicit=allowlist-bypass); #4 new `checkout_done` outcome
rel; #5 `dl --rows <REL>` + `query_rel` daemon RPC.
- [x] **#3 SHIPPED BUG — gh-checkout.dl sweeps ZERO repos against a real config.**
      The example gates `<- repo(slug, root, url), url != ""`, but `refresh_builtin_rels`
      (mod.rs:3484) fills the `repo` rel's url column from `r.url.unwrap_or_default()`,
      and an already-cloned config repo carries slug+root with EMPTY url (url is a
      clone-time hint, dropped once on disk). So the intended deployment (cloned
      staging repos, no url) matches nothing and the example silently produces no
      rows. FIX: drop the `url != ""` gate — head `checkout(slug, "", "0") <-
      repo(slug, _, _).`. Note self is EXCLUDED from `repo` when a config is loaded
      (repo_rows = self.repos only, mod.rs:3484-3490), so sweeping every `repo` row
      is safe (no self hard-reset) in the ghcacher deployment; without a config
      `repo`=self, so add a header warning not to run it config-less (would
      hard-reset the dev checkout). CONSIDER: a `config_repo`/`is_self` marker column
      or rel so the safe filter is expressible, not just "happens to exclude self".
- [x] **#1 repo sink refuses ground facts.** `run_repo_pulls` (mod.rs:3839-3846)
      rejects any literal head term ("head must be all variables (slug, root, url)")
      because it compiles the BODY as a SELECT via `lower_gen` — a bare fact
      `repo("s","/r","u").` has no body, so you must route through an intermediary
      rel. (The `checkout` sink does NOT have this limit — it reads its derived
      table, so ground facts work.) FIX shape: let the repo sink accept literal head
      terms (materialize them directly) the way a normal fact rule does.
- [x] **#2 leading-edge cadence (MISREAD, verify which op).** Complaint: "clock
      without leading edge, waits N." VERIFIED FALSE for `clock`: `refresh_clock`
      (mod.rs:3550) writes bucket=now/secs on EVERY tick incl. tick 0, so
      `clock(300,b)` binds immediately and the first poll fires without waiting. The
      edge-triggered op that DOES wait N is `every(N)`. Likely the session used
      `every` for cadence and wanted fire-now-then-every-N. REAL ask (if any): a
      leading-edge variant of `every`, or doc the clock-not-every pattern louder.
- [x] **#4 demand sink success is invisible in the daemon.** `checkout` DOES
      eprintln one line per repo incl. success (`[checkout] slug: reset main ->
      origin/main`, asserted in tests), but under the daemon stderr goes to
      daemon.log, so a `--load-once` query can't SEE that it fired. FIX: write a
      queryable result rel `checkout_done(repo, branch, action, ok, detail)` (a
      demand sink that also surfaces its outcome as a rel), so the program can react
      (diag on failures) and a live query confirms the sweep. Generalizes: demand
      sinks that do IO should optionally emit an outcome rel.
- [x] **#5 no "print rows of rel X from the live daemon."** Daemon query surface is
      `--load`/`--load-once` (push a script, get query_json back), `--await-settle`.
      To inspect a rel you must write a temp .dl containing `? rel(...)` and
      `--load-once` it; there's no `dl --query 'rel(...)'` one-liner and no plain rel
      dump. Also daemon-vs-oneshot is muddy: three candidate roots, rootless global,
      ad-hoc runs get hijacked by a running daemon, `--no-daemon` was the only
      reliable isolate. FIX shape: a `dl --rows <rel>` (or `--query`) that hits the
      daemon's query_sql RPC and prints rows; document the root/daemon-selection
      rules in one place.

- [x] **Flow-marks + goto-recorder epic F1-F4** (worktree ext-wave3,
      2026-07-10, cb7aacc/2bce1d5/9f03e86, plan
      plans/2026-07-10-flow-marks-goto-recorder.md, 3 Sonnet agents off
      Fable skeletons): F1 = `hover_note(path,line,col,end_line,end_col,md)`
      reserved sink (diag pattern, 0-based, hover merge with --- separators,
      notes-only hover works, guard bails). F2 = `dl/hookEvent` LSP request
      (-> insert_hook_event -> quiet tick; mirrors daemon RPC, thin-client
      forwards verbatim later) + extension `dl.recordFlow` (cmd+alt+g, REC
      status chip, always-on selection sub, jump = Command-kind || file
      change, `last` tracked on non-jump moves). F3 =
      examples/goto-flows.dl: jsonp extract rels (ONE extract op per rel —
      engine law; jsonp paths are dotted, no `$.`; jsonp yields text, cast
      via int()) -> goto_jump -> call_def span lift -> take_edge (same
      event's from/to, no argmax needed) -> flow_take facts + unnamed
      identity default -> flow_union_edge / flow_common_edge (anti-unify =
      count(session) == take count over set-deduped rows) -> flowmark
      panel layer (edge kind = flow name) + hover_note membership +
      ? flow_stat. GOTCHA: named-arg heads need check_and_normalize —
      in-process tests must use prepare_paths, not bare parse::parse.
      Tests: hover_note.rs (3), lsp_hook_event.rs (2), goto_flows.rs (4).
- [x] **Panel harness + zero-rows fix** (worktree ext-wave3, 2026-07-10,
      819bb90+3324451, Sonnet in isolated worktree): vitest+playwright under
      editors/vscode-dl (hermetic fixture bridge answering the dl-bridge
      /rpc shape from canned tables; 9 unit + 6 e2e incl. list/trace
      screenshot baselines, double-run stable; npm test / npm run
      test:e2e). Harness's first run FOUND the wave-2 regression: list
      windowing read offsetTop on the #gutterLeft svg (SVGElement has no
      offsetTop -> NaN bounds -> ZERO rows in any Chromium incl. webview);
      fixed by reading #listRows at all 3 sites, test polyfill removed so
      the suite exercises the real path. The installed vsix carries the bug
      until a rebuild.

### Open (vscode ext review — plan approved 2026-07-10)
Full eval + 3-track design at **plans/2026-07-10-vscode-ext-review.md** (perf
remediation A / references lens B / BOM structure view C, waves 1-4). Staffing:
Opus/Sonnet subagents implement, orchestrator verifies+commits.
- [x] **Wave 1 (A1-A4)** (main 9f4b5b6): dl/query {limit,offset,count} paging
      (page = subquery LIMIT/OFFSET + unpaged total; count mode; legacy shape
      preserved for browser bridge); LSP panel reads no longer write _query_log
      (daemon RPCs still log); `**/*` client watcher DELETED (server has no
      didChangeWatchedFiles handler — events were decoded and dropped); panel
      sends limit = render caps so the wire never exceeds 2k/4k rows (was 20k
      serialized, 18k dropped); linked-only/view-mode toggles re-render from
      cached rows. Suites lib 253/0/1, it lsp 13/0, tsc clean. vsix NOT rebuilt.
- [x] **Wave 2** (main, 2026-07-10, 2 Opus agents): A5 dl/graphChanged {} pulse
      (daemon diag_changed arm + in-process didSave) -> extension forward ->
      panel 250ms-debounced re-run behind default-off "auto" toggle (window
      'message' contract, host-agnostic); A6 list virtualization (ROW_H=22
      exact windowing +10 overscan, full-height spacer, arcs untouched,
      off-screen centering = scrollListToIndex); A7 dead DOM-card renderer +
      Sugiyama + pan/marquee gesture system DELETED (-413 lines net) with A8
      canvas interactivity rebuilt on cy API (mouseover hover card, class-
      toggle pins/highlight, sym-EQUALITY cy.animate centering, tap-to-open);
      B1 Engine::refs_lens (tiers resolved/textual; SCIP tier = wave 3) +
      dl/refs request + textDocument/references rewired w/ multi-repo fix
      (every hit's URI from its OWN repo root via repo_roots) + dlReferences
      TreeView (tier->repo->role, dl.findReferences cmd+alt+r). BONUS fix:
      madge oracle went red on clean main (madge/chalk now colorizes non-tty
      stdout, ANSI codes broke the --warning text parse) — NO_COLOR/
      FORCE_COLOR=0 env + strip_ansi in oracle_madge.rs, green again. Suites
      lib 253/0/1, it full green (daemon scale test load-flaky under a
      parallel full run, passes solo). tsc clean. vsix NOT
      rebuilt. PORTABILITY LAW (Chris): flow-panel.html stays host-agnostic —
      window.dlHost {query,hover,open} + window messages are the ONLY host
      coupling (panel reuse planned in ~/projects/instant).
- [x] **Wave 3 B2+C1+C2** (worktree ext-wave3, 2026-07-10, 230f843+1e87546,
      2 Opus agents): B2 = documentHighlight (same-string spans in-file) /
      workspaceSymbol (LIKE-contains over type_entity+call_def, ESCAPE '\',
      prefix-first, cap 200, per-repo URIs) / documentSymbol (type_entity
      nested by parent) — SymbolRow + 3 engine methods + like_contains,
      tests/it/lsp_symbols.rs. C1 = .dl/bom.dl bom_node/bom_edge (fan_in/
      fan_out distinct via set-deduped bom_ref union of type_link+call_edge,
      negation-guarded zero-default splits, verified 1:1 with member_node =
      12111 rows live) + bomTable preset + numeric band in rowHtml
      (windowing untouched) + sort chips re-sorting from lastKeptNodes
      without re-query. C2 = applyCollapse rollup (subtree totals on
      collapsed rows) + alt-click where-used overlay (callers / type refs
      by kind / field fill-read / importers, all sym-pinned). Rollup unit
      test scripts/test-panel-rollup.mjs. Suites lib 265/0/1, it 613/0/4,
      tsc clean. NOT merged to main (Chris's SCIP worktree in flight).
      Remaining wave 3: B3 (BLOCKED on Chris's SCIP work), A9 (subsumed by
      the thin-client plan), A10 (folding into the vitest/playwright
      harness arc, Sonnet agent running in its own worktree).
- [ ] **Wave 4**: B4 dl/locate follow-the-user; B5 call/type hierarchy; C3
      exploded stratum view (welded-subassembly cycle cards); C4 3D iso go/no-go.
- [ ] **LSP thin client over the daemon (Gradle model)** — plan approved
      2026-07-10 at plans/2026-07-10-lsp-thin-client-daemon.md: `--lsp` becomes
      a stdio<->socket adapter (LspPump mirrors mcp::Pump), in-process engine
      demoted to the --no-daemon arm; new RPCs refs/saved/repo_roots/diag_mute
      + {path,byte} widening of definition/hover/diag; reconnect =
      ensure_daemon so the build_id fingerprint respawns a stale daemon
      mid-session; subsumes wave-3 A9; retires the served-copy divergence
      class. Stages T1-T5, ~M total. LAW to land with it: a mode flag binds a
      transport client-side; engine work routes through daemon RPC with a
      Local fallback (Pump-shaped), never a second resident engine on a
      shared db.

### Singleton daemon + registered roots — LANDED (main 06a86c8, 2026-07-10)
Plan plans/2026-07-10-singleton-daemon-registered-roots.md, Opus implementer,
P0-P4 in 6595ad7/2544043/f1ca893 (+06a86c8 straggler test migration). ONE
process at $XDG_STATE_HOME/sprefa, one socket; `Daemon` (socket/subscribers/
shutdown/registry) split from per-root `ServedRoot` (engine/program/watchgate/
tick methods, db at home/roots/<blake3-16hex>/db.sqlite); config view = a
ServedRoot with key=None (one code path). Every root-scoped RPC carries
params.root; `resolve()` auto-add_roots a .dl-owning miss (attach IS
registration, cold tick blocks the caller); add_root/drop_root RPCs +
roots.json replay; nested-root refusal. `dl daemon start` detaches by default
(--foreground = debug), announces the config view from a rootless cwd;
`dl daemon drop <root> [--purge]`; stop stays global; idle exit = ALL roots
idle. Per-root sockets/pids/dbs RETIRED (stale <root>/.dl/daemon.* reaped
loudly); LSP/one-shot/--mcp/--hook route through the singleton. Tests
hermetic via XDG_STATE_HOME sandboxes incl. the disc2 regression
(sandboxed_daemon_never_binds_default_home); the old per-root leak class is
structurally gone. docs/daemon.md rewritten; old <root>/.dl/db NOT imported
(cold-start on first attach, changelogged). One-engine assumptions found+
fixed: per-root state co-mingled in Daemon, watcher_loop blocking recv,
idle/poll loops, home_dir(Some(root)) conflation, program-edit
process::exit(0) respawn (removed — would kill every root; also revealed the
old idle-exit was dead, part of why per-root daemons leaked). Suites at merge:
lib 277/0/1, it 630/0/4. RESIDUALS: (a) LSP main loop doesn't filter
diag_changed by root yet (multi-root editor over-queries, harmless); (b) a
config-repo edit refreshes registered roots' git facts only on their own next
tick; (c) per-root idle eviction deferred (engines stay warm).

### Open (turnkey query surface — plan plans/2026-07-10-turnkey-query-surface.md)
Goal: dl useful from the first command for devs + agents, no .dl authoring.
Tiers: `dl what` meta-query / `dl q` concept verbs / `dl find` schema-driven filter.
- [x] **Items 1-2** (main 50bc092, Opus agent): anchor resolver `src/anchor.rs`
      (classify name|path|path:line, glob->LIKE, resolve_name unions
      type_entity/call_name/scip_name/scip_binding/df_node.var, missing-family
      = zero rows, split_repo_sym; all reads via lower::tbl so the magic-rel
      audit stays green) + `dl what <anchor>` / `dl summary <path>`
      (src/cli/query.rs; daemon-first `what`/`summary` RPCs in src/daemon.rs;
      in-process fallback forces extraction families via a synthetic probe
      program — `?` items flip ExtractFamily::used). scip honesty note when
      scip_def empty. 6 e2e tests/it/what.rs; post-rebase suites 265 lib /
      608 it green.
- [ ] **Item 3**: `dl q <verb>` runner — param injection (synthesized
      `target("...")` fact merged into an embedded program, the --move
      precedent) + verb_catalog meta rel + who-calls/where-defined first.
- [ ] **Next arc**: blast-radius/dependents verbs via run_reaches_pair (NOT
      materialized closure); built-in MCP tools dl.what/dl.verb/dl.rows in the
      --mcp adapter (the decided-unbuilt eval-bridge ledger item); `dl find`
      schema-driven filter over rel_col anchor columns (Tier 3, deferred).
- [ ] **Implementer-debrief pain points** (from the Opus agent, 2026-07-10):
      (a) crate::daemon vs crate::cli::daemon module-name collision is trappy
      (RPC clients in one, verbs/print_rows in the other); (b) no public
      `eng.ensure_families(&[...])` — forcing extraction for a read requires
      ticking a synthetic program (~40 lines of probe scaffolding), and cold
      `dl what` re-extracts the corpus without --db; (c) TS dataflow silently
      sparse (`return lookup()` body -> 0 df_nodes, no doc says per-lang df
      coverage is thin); (d) scip_ref lacks line/col that scip_occurrence/
      scip_binding have — rel choice needs that asymmetry known up front.
      Chris's own dogfood add (env-rel arc): daemon hijacks ad-hoc gen runs
      with no visible signal — wants a loud "daemon is serving this root,
      writes went there / use --no-daemon" warning.

### Open (pseudo-scip coverage — plan plans/2026-07-10-pseudo-scip-coverage.md)
Best syntactic wins per language short of a compiler (Fable research 2026-07-10).
Ranked: H (JS/JSX rides TsTypes, 0.05 KU) + D (import-scoped ambiguity narrowing
in the extract.rs resolve closures, 0.15 KU) first; then Go TypeLang (1.2 KU),
generic def/ref tags tier over SG_LANG_TABLE (0.4 KU), Python TypeLang (1.2+ KU,
type_link scoped out). stack-graphs ruled OUT (frozen upstream, .tsg cost,
foreign resolution model).
- [x] **H+D LANDED** (main eed54cc, Sonnet agent, 2026-07-10): TsTypes matches
      .js/.jsx/.mjs/.cjs, `source_type_for` replaces the 3 dup path_is_tsx
      branches, extract_file_set SQL + narrowing gate `narrow_ambiguous`
      (survive = self/imported/same-dir; RESOLVE only a lone self/imported
      survivor — same-dir-only tie stays bare); module_import_map read once
      per refresh; digest folds module_edge_rev at EVERY rev (committed revs
      have their own import graph, unlike the WORK-only scip fold). Tests
      type_graph_js.rs + resolver_import_narrowing.rs. Post-rebase verify:
      lib 265/0/1, it 613/0/4, oracle precision unchanged (0.78/0.55 module
      drift is pre-existing, confirmed by stash A/B). REAL BUG FOUND+FIXED
      en route: ModuleFamily only ran when the program named a module_* rel,
      so type_link/call_edge-only programs never populated module_edge_rev
      and the narrowing no-op'd silently — `module_rels_needed` now ORs in
      type/call/doc usage (both ExtractFamily::used and tick_paths).
- [ ] **Latent engine gap (found by the H+D test, unfixed)**:
      `enumerate_with_hash`'s mtime+size fast path treats an equal-length
      edit landing in the same fs timestamp tick as unchanged (test worked
      around with a length-varying marker; any rapid two-tick same-db test
      with an equal-length edit is a flake risk).
- [x] **Aliased/default imports at the SYNTACTIC tier — LANDED** (main d9160ff,
      Opus plan plans/2026-07-10-module-binding-alias.md + Sonnet impl,
      2026-07-10): `module_binding_rev(file, local, source, dst, rev)` +
      `module_binding` (rev-deduped union) capture aliased-import local
      bindings from the EXISTING module-resolver parse (Rust `use x::y as z`
      via UseLeaf.alias, TS/JS `import { a as b }`/default via string-level
      parse_ts_import_clause, Kotlin `import a.b.C as D` regex group; carrier
      = ModuleRef.bindings Vec<(local,source)>). Alias hop in BOTH resolve
      closures: after SCIP override, before by_name, fires only when the
      referencing file declares no same-named def (local shadows import),
      dst-pinned, NEVER falls through to by_name on a miss (honest bare beats
      a coincidental global match). Digest folds module_binding_rev beside
      module_edge_rev (with_scip = "resolver reads outside inputs");
      REV_TWINS + all five module write paths wired. anchor.rs unions
      module_binding⋈type_entity so `dl what <alias>` resolves index-free.
      Tests: 3 modgraph units + tests/it/resolver_import_alias.rs (5 e2e:
      rust alias, dl what, shadowing negative, rev flip, TS alias). Suites
      lib 268/0/1, it 618/0/4. NON-GOALS (honest bare): barrel re-exports,
      namespace/wildcard imports, default-import RESOLUTION (rows emitted
      source="default" but typegraph has no default-export entity — future
      bridge), no repo column (same module-graph residual).
- [ ] **Sonnet-implementer debrief pains** (2026-07-10): (a)
      sprefa-v5-working-conventions skill still shows a `--root` flag that no
      longer exists (cost a round of confused testing; skill source =
      ~/projects/claude-research/skills, backprop candidate); (b) ambient
      ~/.config/sprefa/config.toml silently joins every ad-hoc `dl` run
      ("[config] 3 repo(s) registered") — set SPREFA_CONFIG explicitly for
      smoke tests, nothing documents this habit outside resolver_repo_scope.rs;
      (c) cross-family hidden dependency (resolver reads module_edge_rev) is
      a shape the magic-rel audit doesn't cover — two ExtractFamily used()
      gates coupling invisibly.
- [x] **SCIP-parity twins (TS + Kotlin)** (main 34cade3, Sonnet agent,
      2026-07-10): shared scorer tests/it/oracle_parity.rs (oracle_rust rebased
      onto it, behavior-identical; the ±1 `wrong` drift between runs is RA's own
      indexing jitter, reproduced pre-refactor); TS twin vs scip-typescript on
      new tests/fixtures/ts_ws = 16.7% parity / 1.000 precision (1 confirmed /
      5 bare); Kotlin twin vs scip-java runtime-skips (no JDK in this env — skip
      path exercised, real numbers pending a JDK box). KNOWN SCORER GAP, all 3
      langs, left unfixed to keep Rust scoring byte-identical: the picks key
      derives the callee name from the RESOLVED sym while call_site text is the
      bare as-written name, so correctly-resolved METHOD and ALIASED calls score
      `bare` (under-report, the safe direction). Follow-up: key picks on the
      call site's own text.
- [x] **LANG-JUNCTION skill + rail** (main 3933afa, 2026-07-10): 8 per-language
      registration points carry `// LANG-JUNCTION(slug): what to wire` markers
      (typelang-registry, extract-file-set, module-resolvers, sg-grammars,
      ast-grammars, comment-cst-extensions, scip-indexers, move-rewriter);
      examples/gen-lang-skill.dl regenerates the junction list in
      .agents/skills/sprf-add-language/SKILL.md via comment_node
      (string-literal-safe) with lang-junction-drift/-orphan --check rails
      (slug-set desync fails, line drift alone never does);
      tests/it/lang_skill_gen.rs (4, incl. the string-literal negative).
      scip-go install hint corrected to github.com/scip-code.
- [x] **Go TypeLang + GoResolver LANDED** (main 8ef6cb9, Sonnet agent,
      2026-07-10, pseudo-scip item B): GoTypes one-parse tree-sitter walk
      (struct/interface/alias/function/method entities w/ receiver-typed
      parents via go_owner_kinds; field/impl/generic edges — interface embeds
      AND generic type-set constraints both land as impl, tree-sitter-go 0.25
      unifies them under type_elem; go_fn_type arrows w/ multi-value ret
      flattened; full df lift incl. func-literal ::closure:: scopes, composite
      literals as new+df_field, per-value ret nodes; godoc block docs,
      deprecated tag only). GoResolver = go.mod module line +
      one-package-per-dir, import -> whole-dir .go fan-out (Kotlin wildcard
      precedent), alias imports -> module_binding, blank/dot imports handled.
      collect_manifests gained "go.mod". 8 typegraph + 4 modgraph units, 5 e2e
      tests/it/go.rs. Suites at merge: lib 287/0/1, it 641/0/5.
- [x] **Python TypeLang + PyResolver LANDED** (main 646fc74, Sonnet agent,
      2026-07-10, pseudo-scip item C): PyTypes one-parse tree-sitter walk
      (module/class/function/method entities — NEW EntityKind::Module so a
      module docstring has a type_entity row; annotation-only edges: bases ->
      impl, annotated attrs -> field, params/returns/uses; subscripted
      annotations recurse to the inner ref; PEP 257 docstrings + Sphinx
      :param:/:returns: tags; df lift w/ ctor `new` on capitalized calls,
      kwargs -> df_field, lambda/nested-def ::closure:: scopes, comprehension
      loop spans, self/cls skipped). PyResolver per the mid-flight amendment:
      import-root DISCOVERY from the scanned file set (repo root + src-layout
      + top-level package parents, rspath::crate_roots precedent; multi-root
      ambiguity stays unresolved loudly); sys.path.insert/append counted
      loudly per refresh, never followed; star imports loud-unresolved;
      import-as/from-import bindings. NON-GOALS: attribute-chain resolution
      (type_link scoped out), Google docstrings, forward-ref string
      annotations, `py_root(path)` user-fact seam (deferred — needs the
      declared demand-sink treatment). MERGE NOTE: rebase over the Go arc
      needed hand-merging (both langs appended at the same typegraph/modgraph
      anchors); resolved by re-applying the Python diff onto the Go version at
      unique anchors, suites green post-rebase. Suites at merge: lib 301/0/1,
      it 644/0/5.
- [x] **Go + Python parity twins LANDED** (main f911f1b, fresh Opus agent,
      2026-07-10): tests/it/oracle_go.rs + oracle_python.rs on the shared
      scorer (oracle_parity.rs byte-untouched, TS twin re-verified identical);
      fixtures go_ws (go.mod module, api/util/service pkgs) + py_ws
      (pkg/__init__ layout), each w/ cross-file call, method call, aliased
      import, ambiguous name. MEASURED (both indexers present): Go 75.0%
      parity / 1.000 precision (6 confirmed / 0 wrong / 2 bare); Python 37.5%
      / 1.000 (3/0/5). Bare buckets = the documented scorer gap (method +
      aliased calls key on resolved-sym name). Twins runtime-skip without
      scip-go/scip-python on PATH (SPREFA_SCIP_* overrides; scip-go at
      ~/go/bin, scip-python via nvm). scip-python GOTCHAS learned: needs
      --project-name/--project-version outside a git repo AND exits 0 on
      fatal errors leaving a header-only index — the test skips on an
      empty-document index too. MERGE NOTE: agent's worktree spawned from
      ext-wave3 (not main) so its branch carried foreign merge history —
      landed by cherry-pick of the single commit, ext-wave3 untouched.
      Suites on main: lib 301/0/1, it 646/0/5. Opus debrief pains: (a) the
      parity scorer's expected `?` query blocks (5-ary call_site + call_edge)
      aren't documented at the scorer — one comment would save the
      verify-by-hand round-trip; (b) pre-commit hook prints info[op-example]
      noise to stdout mid-commit, benign but alarming.
- [x] **Scorer per-site keying LANDED** (main 0c3b367 cherry-pick of Opus
      worktree commit, 2026-07-10): plan's Design A (call_target) rejected by
      the agent with reasons — call_target's name-equality pin structurally
      excludes ALIASED calls, and its df_node dependency would regress plain
      calls in TS class-method bodies (zero df nodes there). Design B = new
      `site_pick` rel per twin keying picks on (file, as-written text,
      1-based line), resolved per-site from shipped rels: call_site ⋈
      call_edge ⋈ call_name ⋈ call_def.file, plus module_binding.dst for
      aliases; ambiguous -> multi -> excluded; the single 1-based->0-based
      conversion lives in `score`, commented. BEFORE/AFTER: Rust 51.0->77.9%
      (0.996 precision), TS 16.7->50.0%, Go 75.0->87.5%, Python 37.5->75.0%
      (1.000 each), Kotlin still skips (no JDK). New Rust wrongs 12->17 ALL
      enumerated: per-caller single-def homonym picks (`run`/`push`/`walk`/
      `tick_paths`/`serve`/`dirty` defined in multiple files) — real
      resolver disagreements surfaced FROM bare, kept in wrong, no massaging.
      Debrief pains ledgered below (dl query row indent, call_target header
      note, df coverage table, call_def.sym doc). GOTCHA found: dl query
      data rows carry a 2-space indent — cell 0 of any parsed rel needs
      .trim() (the old scorer worked by accident, file lived in cell 3).
- [ ] **Change-cost friction inventory** (2026-07-10): consolidated 10-agent
      debrief ranking at plans/2026-07-10-change-cost-friction-inventory.md
      (12 items, fix shapes + sizes + sequencing; top = ambient-config
      hermeticity, declared cross-family reads edges, query --format=json,
      the engine monolith epic, resolution_source column).
- [x] **Occurrence-level scip resolution LANDED** (main 7191bc6, Opus agent,
      2026-07-10): `ScipOccIndex` built once per call-family refresh from
      scip_def + scip_occurrence + scip_binding (one SQL pass each, via tbl);
      `resolve_callee` consults position FIRST — occurrences at
      (repo, file, line-1) filtered to the as-written call text (descriptor
      name or aliased local binding), one survivor resolves, same-line
      same-name refuses, miss falls back to the name map. The single
      1-based->0-based conversion lives in ScipOccIndex::resolve, commented.
      Corpus with-scip: rust 27.5->33.0% (0.974), go 89.0->93.3%, python
      78.0->79.3%, ts flat (defs outside scan root); without-scip arms
      byte-unchanged. Type resolver stays name-level (TypeEdge/type_sig refs
      carry no source position — documented deviation). scip_gate.rs
      rewritten: same-name calls on different lines resolve to their OWN
      defs by ranged occurrences; same-line conflict refusal + no-occurrence
      name-map fallback tests added. SIDE ANSWER: pre_extract (84ae5d7) DID
      cut the scip_want demand chain 3->2 ticks — asserted in
      scip_want_call_resolution_lands_on_tick_two. Remaining ceiling =
      def-in-scan-corpus + dl's own site detection (macros/UFCS). GOTCHA for
      test authors: rangeless occurrences are skipped by scip_occurrence
      (parse_range None) while still feeding scip_def/scip_ref — occurrence
      tests must supply .range.
- [x] **String-values arc (df_lit + const_value + std/strings.dl)** (main
      48db929, Sonnet agent, 2026-07-10, plan
      plans/2026-07-10-string-values-const-value.md): `df_lit(id, text, kind)`
      + `df_lit_rev` (dataflow family; text = cooked literal / raw template
      slice, kind lit|template|concat; TS/JS + Rust syn::Lit::Str); NEW
      `concat` df_node kind (TS `a + b` edges both operands in, was an
      unchased expr); `const_value(repo, sym, field, text, kind, file, line)`
      + `_rev` (type family, rides TypeFacts like doc_comment; dotted field
      paths for object literals, string enum members off the enum sym,
      let/var EXCLUDED loudly — soundness rule; mints the previously-unbuilt
      EntityKind::Const type_entity row for string-bearing consts only);
      std/strings.dl string_flow/string_flow_trace (recursive rel seeded from
      df_lit, flow-ctor pattern) + examples/string-values.dl (route-table
      shape). New CONST_VALUE_RELS family gate. Tests: 12 typegraph units,
      const_value.rs (5), string_flow.rs (2). Suites at merge lib 317 / it
      694, magic-rel audit exit 0. LEDGERED: Kotlin/Go/Python df_lit +
      const_value; type-ref positions (type resolver stays name-level — see
      occurrence-level entry). Implementer pains: ts_flow_expr threads starts
      but not content (raw-slice access needs a workaround field); family-gate
      wiring scattered across 3 files with no single checklist point;
      rel_rows returns empty identically for undeclared rel vs un-fired gate.
      OVERLAP WARNING: a parallel session landed/lands const_string_member +
      template_parts + import_binding(_rev) — const_string_member vs
      const_value and template_parts vs df_lit(kind=template) cover
      overlapping ground with different shapes; needs a reconcile pass
      (union, dedupe, or documented split) once both are on main.
- [ ] **Daemon scip index staleness** (verified 2026-07-10, unfixed — P2 of
      the scip damage plan): index.scip is gitignored (.gitignore:18), the
      watchgate drops gitignored events, so ScipKind::dirty ("index.scip in
      the changed set") never fires in a running daemon — a rebuilt index
      stays stale until an unrelated full tick. Fix shape: allowlist
      index.scip paths through the watchgate like the .git ref allowlist
      (S); optionally `dl index` pokes the daemon on completion.
- [ ] **Scorer-agent debrief pains** (Opus, 2026-07-10, unactioned): (a) dl
      query output indents data rows 2 spaces — undocumented, mis-keys any
      parser reading cell 0 (document in the query-output contract); (b)
      std/flow.dl call_target header should state it is name-equality-pinned
      (aliases excluded by design) and df-dependent; (c) per-language df
      coverage is invisible (TS class-method bodies emit ZERO df nodes —
      third sighting of "TS dataflow silently sparse"); (d) call_def.sym doc
      says bare "file::kind::name" but the emitted value is repo-qualified.
- [x] **Otel parity corpora + real-corpus measurement** (main 12b5a25 pins,
      df27369 harness, 2026-07-10): five otel repos pinned as submodules at
      release SHAs under bench/corpus/ (rust v0.31.0 / js v2.9.0 / go
      v1.44.0 / python v1.43.0 / kotlin=android v1.5.1, opt-in submodule
      init). tests/it/oracle_corpus.rs = five #[ignore] two-arm tests
      (SPREFA_CORPUS_DIR; worktrees have empty submodules -> loud skip).
      FINAL NUMBERS (post-fixes): index-free rust 14.1%/0.945 (trait-call
      bare-dominated, 5218/6131) / ts 20.4% (core pkg, bare = out-of-root
      ../../api imports) / python 40.1% / go 72.7%; WITH scip rust
      27.5%/0.976, python 78.0%/0.996 (index REMOVES 10 syntactic wrongs),
      go 89.0%/0.995, ts unchanged (honest: defs outside scan root).
      Sprefa's own 77.9% vs otel-rust 14.1% = corpus shape (monolith free
      fns vs trait-heavy API), the real per-lang ranking inverts the
      fixtures. HAZARD FOUND en route: enumerate_with_hash walked submodule
      contents (200MB entered every **-glob scan + daemon watch; pre-commit
      hung) — gitignore fence first, then NATIVE nested-repo pruning landed
      (3bdcce5): walker + watchgate prune any depth>=1 dir owning a .git
      entry, e2e both .git forms; fence kept for the old running daemon
      image. Agent debriefs: corpus scan of a submodule root HANGS pre-tick
      on the gitlink .git file (dl should fail loudly, unfixed); worktree
      agents must get the base SHA in the brief (main was unpushed).
- [x] **SCIP one-shot no-op: gate + tick order + override ambiguity**
      (main, 2026-07-10, found BY the corpus with-scip arm measuring
      byte-identical): (1) ScipKind::used gated on the program NAMING a
      scip rel while the type/call resolvers read scip_ref — SPREFA_SCIP_INDEX
      was a silent no-op for exactly the programs it should improve
      (ModuleFamily gate bug shape); used() now ORs TypeFamily/CallFamily.
      (2) index loaded in the RelKind loop AFTER extract families — fresh-db
      tick 1 extracted index-blind, healed tick 2 via digest fold; new
      RelKind::pre_extract hook runs scip before extraction in tick +
      tick_paths. Proof: otel-go/sdk call-only program fresh db first tick
      3734 -> 4041 call_edge rows. (3) The fixed arm instantly exposed
      scip_name_defs last-write-wins: a file referencing two symbols both
      named `build` clobbered -> 412 wrong picks on otel-rust, precision
      0.819; the override now DROPS a (repo,file,name) carried by two
      different def symbols (fails toward exclusion; ~3 parity points for
      precision 0.976-0.996). tests/it/scip_gate.rs (3: gate+ordering
      positive, index-free bare control, conflict refusal).
- [ ] **Go/Python implementer debriefs** (2 Sonnet agents, 2026-07-10):
      GOOD both: field-based tree-sitter grammars (go, python expose
      child_by_field_name) made both extractors shorter than Kotlin's
      positional-child matching; the TypeLang registry + one extract_file_set
      SQL clause wired call/df/doc with zero extra engine work (architecture
      behaving as designed); the LANG-JUNCTION skill turned "grep for %.kt"
      into a machine-generated checklist mid-arc (Python agent used the map
      the same day it shipped). PAINS: (a) tree-sitter-go 0.25 gives interface
      embeds and generic type-set constraints the SAME `type_elem` node kind —
      distinguishing them needed grammar-source reading; ASK = per-grammar
      node-kind reference (node-types.json digests) as a skill asset for
      future TypeLangs. (b) EntityKind exhaustive-match discoverability —
      FIXED same day: doc note "matched exhaustively only in tag()" added at
      the decl.
- [ ] **Orchestrator codebase pains** (Fable, 2026-07-10, from the skill+rail
      arc): (a) rel LINE BASES are lore, not docs — comment_node is 1-based,
      scip_occurrence 0-based, df 1-based; learning comment_node's base meant
      reading cst.rs source. RelDecl doc strings should state the base per
      positional rel (one regen sweep). (b) the two grammar tables live in
      inconsistent homes: SG_LANG_TABLE in src/sg.rs, AST_LANG_TABLE buried at
      line ~7674 of the 7798-line src/engine/mod.rs — a lang-support table
      inside the engine monolith is placement debt (engine refactor epic
      context; the LANG-JUNCTION map now at least finds it). (c) +1 to the
      ambient-config pain: every ad-hoc `dl` run at this root prints
      "[config] 3 repo(s) registered" and ingests type/call/doc for repos the
      program never mentions — hermetic needs SPREFA_CONFIG set by hand.
      (d) S6 (body-level source+derived mix silently drops the rel atom) cost
      a failing-test loop to discover; the rel-level guard set the expectation
      the body-level case would also bail.

### Open (scip / language-surface — 2026-07-08 agent-session feedback, batch 2)
Five more complaints (SCIP + dl-surface). NOT yet triaged against code; capture only.
- [x] **S1 scip_ref has no line/column** — FIXED in v0.6.24: `scip_occurrence(file,
      symbol, line, col, end_line, end_col, role, repo)` carries every occurrence's
      0-based span + role (src/rels/scip.rs:62).
- [x] **S2 scip_name returns the canonical export name, not the local binding** —
      FIXED in v0.6.24: `scip_binding(file, symbol, local_name, line, col, repo)`
      joins an occurrence's local source slice (alias/default import) to the
      canonical symbol (src/rels/scip.rs:68).
- [ ] **S3 computed values cannot bind in the body (`x = replace(...)`).** Must inline
      into the HEAD; error message is good but surprises SQL/datalog-with-assignment
      intuition, and nesting `replace(split(...))` in the head hurts readability. ASK:
      body-level bind for pure-fn values (Call/Arith binding position, ast.rs), or
      accept the cost.
- [ ] **S4 no `+` operator for strings.** Concat only via template interp, which reads
      oddly for URL building. ASK: a `concat(...)` fn or `+` on text -> SQLite `||`.
- [ ] **S5 ast-grep patterns are exact-shape.** `{ element: <$C/> }` matched nothing
      (shape strictness / metavar-in-JSX). Repro + narrow: grammar, intended match,
      sg pattern-compilation limit vs grammar gap. S1/S2 pair is the SCIP crux.
- [x] **S6 source-extract rule body silently drops an extra rel atom** — LANDED
      2026-07-18 (3b2319a6, eaten-diag arc): now a typecheck ERROR
      `source-rule-extra-atom` ("put the source-extract rule and the join in
      two separate relations"), verified firing live 2026-07-19.
- [ ] **S7 two source rules silently merge across `use`** (Chris, 2026-07-10,
      salvaged from ext-wave3 worktree 2026-07-19): same-named rels from a
      `use`d module and the local program (or two modules) silently UNION — no
      shadowing, no warning, no way to say which one you meant. Need module
      rename/alias/specific-import syntax (`use std/flow as f`,
      `use std/flow (call_node)` shapes TBD); silent cross-module union must
      at minimum warn.
- [ ] **S8 error-message gap: gen splice with l0==l1 errors** (Chris,
      2026-07-10, salvaged 2026-07-19): a zero-height gen splice span errors,
      forcing every json marker in template.html to become a 3-line block;
      discoverable only by hitting it. `r#""` fixes it. Fix: either support
      l0==l1 single-line markers or make the error NAME the r#"" fix
      (paste-ready-errors standard, small-model-surfaces item 4).
- [ ] **S9 doc gaps** (Chris, 2026-07-10, salvaged 2026-07-19): (a) bool is
      not a usable column brand — flags travel as "true"/"false" text and must
      re-embed with `json(flag)`; (b) arithmetic like `line+1` must live in a
      rule HEAD, never a body binding (S3's computed-value-bind wart, restated
      for docs). Both belong in the reference/book + the authoring skill until
      the surface itself changes.
- NOTE (2026-07-19 salvage): ext-wave3 also carried a P1/P2/P3 --check perf
  RCA (full accounting now at docs/perf/2026-07-10-check-time-accounting.md).
  Largely superseded by the 2026-07-18 wave: P1 empty-derived full-rebuild →
  digest-before-write + `_derived_complete` marker-ensure (e6975f7a); P2
  single-writer cache.db flood → two-worlds L2 one-db-per-root (6f63eaf5);
  P3 missing phase records → perf.jsonl now carries pid + derived phase rows.
  P1 residual checked 2026-07-19: `any_derived_empty` and its per-rel
  COUNT(*) probes no longer exist (survives only in comments — replaced by
  the `_derived_complete` marker system, src/engine/meta.rs:1157). P1-P3 all
  closed.

### Style notes for this repo
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.

---

# Archived 2026-07-18 PM pare (verbatim ledger snapshot before the handoff rewrite)

## v5 Work — Tasks Context

The recurring debt we keep re-hitting has two shapes: **(1) per-row write loops
(N+1)** and **(2) bespoke per-relation refresh functions**. A third,
**(3) string-inline-everywhere**, is the ref-spine debt.

Open items below are one-liners; full history + landed detail in the archive.

### Features / arcs
- [ ] **Auto-architect vision** — the umbrella doc for the capability ladder (facts -> measures -> effect coloring -> lock/channel interval analysis -> auto-suggested refactor seams by coupling+affinity), SOTA anchors, dogfood-first validation law: **docs/vision-auto-architect.md** (2026-07-18). Children: callable completeness landed; effect inventory written (docs/effect-inventory.md); decomposition plan written (plans/2026-07-18-decomposition-normalization.md, execution open); resource-aware scheduler plan written (plans/2026-07-18-resource-aware-scheduler.md, b78334f2 — 7 open decisions w/ recs, 6-step migration, verdict: adopt petgraph, build scheduling on jobq; execution open).
- [x] **Callable completeness landed** (bac35f31..2cc13510): EntityKind::Lambda + CallKind::Closure renamed Lambda (was zero-emitter); lambda syms via shared `lambda_sym` helper = byte-exact with df closure syms (anti-join pinned); flips: Rust {nested fn, trait decl, trait default, closure}, TS {ctor, nested fn, unbound lambda}, Kotlin {ctor, lambda}, Go {func literal}, Python {lambda}; `new X()` call sites resolve to `<Class>.constructor` call_defs. Self-verifying rail examples/callable-coverage.dl green (15/15 claimed=present, 48 fixture rows, diags 0). Two-tier finding: rust-analyzer scip emits NO symbols for closures (0/199; named fns 100%) — diet tier exceeds scip for anonymous callables. Suites it 872/0, lib 549/0. Residual gaps documented: TS object-literal/prototype/export-default-anon methods, Kotlin accessors, Go interface specs, Rust const/static/macro bodies.
- [x] **doc-marks landed** (b6c9fe7d, K3-authored): `@@doc <slug> <content>` in any hooked chat message routes into docs/marks/<slug>.md via gen {var} path fan-out; smoke-tested end-to-end. Activation = the program must be served on the root (daemon renders gens; cold in-process --hook strips them).
- [ ] **Auto-refactor**, rides C: `edit(ref_id, new_string_id)` sink, `--fix`/LSP rename. Route A (`--move`) landed; residual = brace-head `use crate::{clk::X, ..}` + physical file move + moved file's own imports. Plan: `plans/2026-05-31-auto-refactor-use-path-rewrite.md`.
- [ ] **vscode Wave 4**: B4 dl/locate follow-user; B5 call/type hierarchy; C3 exploded stratum view; C4 3D iso go/no-go. Plan: `plans/2026-07-10-vscode-ext-review.md`.
- [ ] **LSP thin client over the daemon**: `--lsp` = stdio<->socket adapter (LspPump mirrors mcp::Pump); retires served-copy divergence. Plan: `plans/2026-07-10-lsp-thin-client-daemon.md`.
- [ ] **Turnkey query surface**: `dl q <verb>` runner (param injection + verb_catalog); then blast-radius/dependents verbs via run_reaches_pair + built-in MCP tools dl.what/dl.verb/dl.rows; `dl find` (Tier 3). Plan: `plans/2026-07-10-turnkey-query-surface.md`.
- [ ] Migrate deck graph (`examples/anim-self.dl` + anim AtlasPanel) from name-keyed `type_edge` to sym-keyed `type_link` + `type_entity` (optional; changes node identity).

### Bugs / gaps
- [ ] **2026-07-18 kill-respawn read-storm incident** (failure-modes classes 16+17): external agent's doubled `dl --check` + user kills = 6 singleton generations in 7min, each blank-slate ("first-run"; deferred digest saves persist nothing on kill), each reading 400-500MB against a 979MB root db (7.3MB corpus, ~140x). Done: VACUUM (979→895MB — bloat is live rows: `_strings` 187MB w/ index, 463 tables autoindex-doubled), 1.9GB per-root fossil cache.db deleted; 2026-07-18 PM user deleted the installed binary + ALL state dbs (singleton, roots/, jobq, invlog) — clean slate, daemon DOWN until a governor+parse-once binary is deliberately installed. Open rails: `--check` client autostart-once+backoff (--hook is attach-only now, class 18), mid-cold digest persistence + kill-mid-cold resume it-test, db-size ratio verdict+ceiling.
- [x] **parse amplification + CPU governor landed** (2026-07-18 PM, failure-modes class 18): source extract made one job per (file, RULE) — K ast rules = K tree-sitter parses of the same file, both full-batch and work-path shapes. Fixed: per-file jobs + `AstTreeCache` shared per (file, grammar) (`ts_lang_resolved` canonical label), `AST_PARSE_COUNT` counter, fail-pre-fix parse-counter tests; receipt 636→159 parses (4x), ~35% cpu on a 5-rule/159-file program; staged bytes/determinism pinned unchanged. Plus src/budget.rs duty-cycle governor (OS tiers are advice, not ceilings — 278% CPU with all tiers applied): daemon caps at 100% default, `DL_MAX_CPU_PCT`/`DL_NO_BUDGET`, throttle_point() at work boundaries, toggle receipt 1.10x→0.87x. Plus `dl --hook` self-deadline: `DL_HOOK_DEADLINE_MS` (default 900ms), on expiry no-op + exit 0 + warn trace + invlog-closed row; cold-db hook = immediate no-op; hook path attach-only (never autostarts). Residual: daemon-side mid-tick cancellation on client drop (req_id still None), sg/ast_yaml internal ast-grep tree not shared with AstTreeCache.
- [x] **obs-logging landed** (55f2d252..ea3536cd + 76822bd0, merged 2026-07-18): src/invlog.rs global invocation log ($XDG_STATE_HOME/sprefa/invocations.db — SQL row per CLI entry incl. --no-daemon, 5-deep ancestry, open row = killed, `dl daemon invocations` verb, why joins dead-pid spawner); rolling dl.log/error.log tracing layers (hand-rolled on why.jsonl rotation pattern — tracing-appender rejected: no size rotation, unstable filenames; analysis in plans/2026-07-18-observability.md); src/reqid.rs per-request ids on HTTP+socket w/ access-log lines; JobRow.req_id plumbed but always None today (no RPC verb synchronously enqueues; ticks originate outside request scope — residual documented in plan layer 4). Suites: lib 555/0 (x3 parallel), it 881/0. Flake fixed en route: activity slot tests serialized behind SLOT_LOCK (76822bd0). NOT LIVE until `cargo install` (deliberate binary swap = one cold rebuild; class 16 caution).
- [ ] **storage-diet arc** (plan 64faea11 + user rulings 7f351d89: plans/2026-07-18-storage-diet.md): db is 60% index bytes (562/937MB); projected 123x -> ~52-66x. Rulings: A=1a dense dictionary, C=both index levers, D=drop norm, B/E per rec. STEPS 1+3 LANDED (6235a233): _strings.norm column + 67MB dead index dropped, idempotent open-path migration (DROP INDEX then gated DROP COLUMN, SQLite 3.45), `string` rel third col = query-time sprf_norm(content) byte-identical; 5 new it-tests incl old-schema migration + byte receipt (fixture: -60% _strings footprint); suites 555/0 + 886/0. NEXT: step 2 index audit (EXPLAIN receipts, demand-aware auto_indexes, 150-220MB), then 4a WITHOUT ROWID junctions; step 5+ coordinates w/ ref-spine + decomp (typegraph.rs locality).
- [x] **rusqlite containment rail landed** (22293133): .dl/no-new-rusqlite.dl ratchet — seam = db.rs+storage*, 30 files grandfathered at measured counts (236 sites), `@rusqlite-ok:` waiver, red-proven in sandbox both codes + waiver-clear. Discipline: shrink a file's count -> lower its baseline row same commit. Seam stays a STRUCT (user ruling); trait only when a second backend arrives.
- [ ] **Erase public no-daemon split** (user directive 2026-07-18): the daemon is an HTTP server; an inline/"no-daemon" run must be the same server on a random port running the same request path — one code path, one logging discipline. `--no-daemon` survives only as an internal test-harness mode; remove it from all public-facing docs/help/flags. Also erases the two-db-worlds split (singleton roots/<hash>/db.sqlite vs per-root .dl/.state/cache.db) and the empty-scan sharp edge's surface. Sequenced after obs-logging lands (touches the it-suite broadly).
- [x] **daemon cpu hog root-caused and fixed**: 2s poll full-tick storm (7af0e319) + per-process exe-identity cache forcing full corpus rebuilds every tick of the first post-install daemon (c351ed90). Ledger folklore about restart-after-install retired.
- [x] **exe-swap boot write storm root-caused and fixed**: nondeterministic extraction (file-set SELECTs in rowid order + cached-facts hits-then-misses feeding first-wins id dedups) made every re-extract emit different rows, honestly re-triggering the full derived cascade. Deterministic order landed (80617b6b); double-swap receipt 2026-07-17: 4.7GB → 111MB, cpu 72.9s → 8.5s. Crash-window per-component wipe/mark + deferred digest saves + bulk-rebuild I/O mode landed (7f4d9c58, 6afd2cf3, 5cf4be15); live kill -9 mid-derived receipt 2026-07-17: clean recovery, no storm, follow-up ticks <1s (component-scoping pinned by it-tests). RCA: docs/rca-exe-swap-write-storm.md. Rails: determinism it-test (a45c34d9) + 4 syntax rails w/ prev-rev oracle (792cc902, scripts/rails-oracle.sh).
- [x] **one-shot dl choke fixed** (e7d29829): apply_process_budget on every CLI entry (DL_NO_BUDGET/DL_BUDGET_DEBUG), DL_MAX_WALL_SECS watchdog (default 300s, exit 124 naming phase/root), attach-client 10s heartbeat + exit 75 bound. Live receipt from ~/projects/instant: client 0.03s cpu/10MB, phase-naming waits, 71s cold worst case.
- [x] **program-edit write storm fixed via dirty-rel scoping**: per-rel `drv:` rule-shape digests (derived twin of `src:`) attribute a program edit to the moved heads; the full-layer wipe downgrades to the scoped rebuild seeded with them. Receipt (warm src/rels corpus): edit tick 1 derived row vs 7,312 forced-full. Discriminating tests: tests/it/derived_scope.rs (proven fail-pre-fix), perf_facts contract updated. Residual: UNATTRIBUTABLE full rebuilds (blank slate, carry, edge-list change, crash-recovery derived-missing) still rewrite byte-identical tables in SQL — a content-skip (digest-before-write) would need rows through Rust or shadow-table compare; not landed.
- [x] **effects orphan mystery root-caused and fixed** (67ed59fe): dynamic effect templates were invisible — rel_effect_cmd stores interned INTEGER ids, both executor call sites read them via as_str() (None), so every boot parked dynamic-template effects orphaned. Read rel_effect_cmd_txt instead + unconditional orphan probe in requeue. Boot receipt: 6/6 effects done, 0 orphaned (was 5 re-parked every boot).
- [ ] **empty-scan guard covers no-scan only** (c3c587c9): a program with SOME scan rules still narrows an existing db to its own scope on --no-daemon runs (smashy snapshot 618→68 files). Inherent reconcile semantics; documented sharp edge in docs/arch-measures-review.md.
- [ ] **root attribution residual** (c33ffc04): single tick_root pairing slot mispairs if two ticks begin concurrently on different threads; process-global approximation accepted, job-context plumbing is the true fix.
- [x] **R7 diag stage routing landed** (73dbcc4a): diag_stage builtin sink + `--stage live|commit|agent-turn|agent-session` on --check + hook routing (agent-turn = touched-path gate, agent-session = per-code summary). Defaults: error -> every stage, unrouted warning -> commit only (storm rails stop spamming live/agent surfaces). User decision recorded: tracing crate, never eprintln (DL_TRACE cli / DL_LOG daemon); 223-site eprintln inventory + build-vs-buy analysis in the plan. Editor live surface deferred to vscode Wave 4.
- [ ] **measures verdict decided 2026-07-18: keep ALL** (research instruments; overlap is data). In flight: std/measures.dl with top-K views + review-doc verdict recording. `dl q` verb wiring still rides the unbuilt turnkey-verb-runner arc.
- [x] **enumerate_with_hash racy window closed** (f2205994): git-racy-index-style guard — fast path also requires `mtime < walk_ref_secs` (persisted per-walk reference second, whole-second only), so a same-length same-tick edit rehashes; quiet-tree rehash footprint ~0; fail-pre-fix test in tests/it/racy_mtime.rs.
- [x] **cold-start staging landed** (61878e5a/8829d74c/0ae36735/9aaeccb6): blank-slate daemon boot seeds `_cold_node` per used family, drains as throttled ColdExtract jobq jobs (single-flight), completion tick does the one derived rebuild; kill -9 resumes only pending nodes; --no-daemon stays inline; equivalence + crash-recovery it-tests.
- [x] **cold-start work-chunking landed** (d962ecf2..a201790c): measurement flipped the premise — parse is 4% of cold cost; dataflow was the hog (4.4s: emit 2.3 + write 2.1 over 115k rows), call is a 1.4s corpus-global barrier (honest floor). Dataflow now drains in deterministic 512KiB/64-file contiguous chunks of the byte-sorted file set (DL_COLD_CHUNK_BYTES/FILES); family digest saves once at the completion gate; SCIP is its own highest-priority scip-index node. Longest single job 2468ms -> 766ms (3.2x). Chunked==inline equivalence + crash-mid-chunk resume pinned. Scoped out with numbers: call/type/module/doc (barrier or not hogs), spine (no per-file dimension); comment/template/unresolved ride the same seam as follow-up. This closes the daemon-boot exposure item (staging was chosen over root-dropping levers).
- [x] **deltaflow per-row N+1 batched** (2bda577c): 4 write loops -> chunked multi-row INSERT/DELETE under 800-param ceiling inside the same BEGIN IMMEDIATE; loop at old line 206 is read-only (left per-row, commented); guard test pins write statements not scaling with change count.
- [x] **loop break-value df tails** (aa6722ea): `let x = loop { break v }` flows v -> x; labeled `break 'outer v` resolves through the loop_breaks frame stack; both tests proven fail-pre-fix; sprefa's own corpus has zero value-carrying breaks today (pure correctness closure).
- [ ] **S3** retired — body-level bind for pure-fn values lowers as inlined expr (src/lower.rs `bind_lowers_to_inlined_expr_sql`). **S4** retired — text `+` concat landed (docs/reference/syntax.md:23; heads + comparison sides, never in a binding atom). **S5** ast-grep patterns exact-shape (metavar-in-JSX `{ element: <$C/> }` matched nothing). **S6** source-extract rule body silently drops an extra joined rel atom (rel-level guard doesn't cover body-level mix).

### Debriefs / friction (backprop candidates)
- [ ] **Change-cost friction inventory** — 12 ranked items, fix shapes + sequencing: `plans/2026-07-10-change-cost-friction-inventory.md`. Top: ambient-config hermeticity, declared cross-family read edges, query --format=json, engine-monolith epic, resolution_source column.
- [ ] Recurring pains across agent debriefs: (a) **ambient config** — every ad-hoc `dl` run ingests `~/.config/sprefa/config.toml` repos; set `SPREFA_CONFIG` for hermetic smoke tests. (b) RESOLVED (75245073): line/col base documented per rel in RelDecl docs + docs/reference/relations.md, verified against extractors. (c) RESOLVED (3c0d9141 doc + 60f0847a fix): docs/df-coverage.md maps per-lang df coverage; ts_flow_class arm added — TS/JS class methods (instance/static/ctor/getters/setters) now emit df rows (sample class: 7 -> 38 df_nodes); residual gap = class field initializers (no enclosing fn scope), documented. Getter/setter share one fn sym. (d) RESOLVED (bc7e531f): data-row indent removed from every `?` printer. (e) RESOLVED (0615b7e0): public `Engine::ensure_families(&[&str])`, errors on unknown family, no derived rebuild. (f) `crate::daemon` vs `crate::cli::daemon` collision. (h) AST_LANG_TABLE buried at ~mod.rs:7674 (engine-monolith placement debt). (i) NEW: pre-commit `dl --check` in throwaway worktrees cold-starts a daemon and hangs — every delegated agent hit it; worktree-root detection or a hook fast-path for blank-db roots is the fix shape. Also landed: `dl query --format json` (fedcb388, friction-inventory item).


---

# ARCHIVED 2026-08-02: full CLAUDE.md ledger as of the lane-wave merge (turbo-minimize pass). Verbatim below.

# sprefa

Reactive datalog-over-code engine ("dl"), living at the **repo root** (v5 lifted
2026-07-01): SQLite-welded, facts extracted via `scan`+`regex`/`ast`/`sg`/`json`,
recursive rules lower to a SQL fixpoint. Prior iterations: v3/v4 working trees in
`~/projects/sprefa-archive-20260701` (also full git history); the OG coordinate
model (strings/refs/byte-spans) in `~/projects/sprefa-archive-20260428`.

User-facing overview (model, DSL surface, CLI, examples, known gaps): **`README.md`**.

Deep state lives in auto-memory (`project_v5_dl_engine`, etc.) + `chat_log/` session
logs + `plans/`. This file is the standing task ledger only.

**Completed-arc history** (85 landed items, full detail) lives in
`.agent/memories/sprefa-task-ledger.md` — read it on demand, not auto-loaded. This
file keeps only the standing laws + currently-open work.

## Standing laws (user-set, non-negotiable, apply to every agent at every level)

- **Doubt yourself before asserting** (user-set 2026-07-23): you are a compression
  algorithm, not an oracle; a large share of your confident claims are wrong. Hedge,
  verify against the code, and do not tell Chris what to do as if it were settled. When
  you lack enough info to answer, or he is asking outside his own expertise and needs
  more depth than you hold, SAY SO and go get it (read the code) rather than guessing.
- **Build-vs-buy**: never assert "we should write our own" for any common-shaped
  problem (queues, servers, schedulers, parsers, telemetry) without FIRST running
  library research and presenting a written analysis of the candidates and why each
  does or does not fit. No one-line dismissals of libraries. The analysis comes
  before any bespoke line of code.
- **Self-diagnosis before execution**: the daemon does not run until `dl daemon why`
  can state, from the on-disk trail alone, what it was doing and what it consumed
  (CPU, disk I/O) — including after a SIGKILL or crash. No receipt runs, no smoke
  tests that start the daemon, until that capability is installed. Never make the
  user ask "why is it slow" — the system answers that itself.
- **Nothing seizes the machine**: CPU (QoS/nice), disk I/O (IOPOL_THROTTLE), and
  thread budget are all capped in `apply_daemon_budget`. First-run rebuild included.
  A change that can beachball the machine is a blocking defect, not a follow-up.
- **The 10-second law** (user-set 2026-08-01): at this repo's current scale, any
  operation that takes more than 10 seconds — test, receipt, compile, rail, script —
  is violently wrong, a defect to investigate now, never a budget to normalize. The
  one named exception is SCIP indexing. Caps and budgets state what a thing SHOULD
  take. A program nobody measured does not get to grow slow quietly (the atlas died
  of this). Applies to every agent at every level.
- **The failure ledger is standing** (user-set 2026-07-18): every incident that
  bites us gets an entry in docs/failure-modes.md — incident receipt, law, rail
  status — following its "how a new rail gets born" pipeline (incident -> RCA ->
  fail-pre-fix test -> rail -> entry). No incident closes without its entry. Do
  not rely on skill self-updates to carry this knowledge; the doc is the record.
- **eprintln never comes back** (user-set 2026-07-18 PM): no `eprintln!` ever
  returns to `src/**`. Diagnostics go through `tracing` macros only; the rare
  CLI-UX line that must bypass tracing carries an explicit `@eprintln-ok`
  waiver. `.dl/no-new-eprintln.dl` ratchets the count to zero and the baseline
  never rises. Applies to every agent at every level.
- **Infra is bought, never built** (user-set 2026-07-18 PM, supersedes the
  scheduler plan's build-on-jobq verdict): scheduling, job queue, HTTP serving,
  daemon lifecycle/supervision, and logging/telemetry run on established Rust
  libraries (or the OS service manager). Logging = the `tracing` crate spine —
  new signals land as tracing events/subscribers, never a parallel bespoke
  pipeline (invlog/why/verdict are migration targets onto subscribers).
  Bespoke versions of these subsystems are migration targets, and no new
  bespoke line lands in them beyond keep-the-lights-on fixes. The datalog
  engine core (lowering, fixpoint, extraction) remains the one legitimately
  bespoke layer.

## v5 Work — open items only (landed detail: .agent/memories/sprefa-task-ledger.md)

### In flight
**v5-port + perf-tracing arc** (2026-07-27 late, plans/2026-07-27-v5-port-perf-header.md):
scopes fixtures LANDED (conformance 97 -> 109, merged d481159e); opus diff review
LANDED (plans/2026-07-27-diff-review-findings.md — finding 1 double-fire = USER
ACCEPTED no-fix, 2+3 http fixes dispatched, rest banked); SLOT-LIB filled
(tracingChannel + pino, user approved the pino dep,
plans/2026-07-27-perf-tracing-buy-verdict.md). http fixes 2+3 LANDED (mergeMap
body read + SSE response.end, 2 regression tests, dl 76/76). ghcacher phase 1
LANDED (v6/dl/fixtures/ghcacher.dl ACCEPTED by the server + ghcacher-findings.md,
9 findings F1-F9): HEADLINE = F7 engine crash, first real host response commit
dies `SQLITE_ERROR: no such column: NaN` (1_hosts.ts:491 commit path, statement
text not surfaced by LibsqlError; root cause OPEN, fix agent queued BEHIND the
P0 tracing merge since per-statement tracing surfaces the failing SQL).
PROVEN GAPS awaiting user word (the zero-new-constructs exception clause):
F2 no clock/cadence = the SLOT-SWR-defining gap (spelling A in-language chosen,
B external-cron documented); F3 no json term-extract, array-explode
inexpressible; F8 rel(1) is whole-table sweep + silently inert on rule-headed
rels, Key(text) unimplemented (feeds the Q8/Key ruling); F9 no effect_log rel
(self-diagnosis law gap). F4 confirmed the not_stratified guard fires correctly
on the v5 etag idiom. P0 tracing spine LANDED (0_trace.ts: tracingChannel +
pino, DL_PERF_LOG opt-in, one JSONL line/tick, overhead -0.02% within noise,
dl 79/79; ratchet filter tightened to Channel\.subscribe call shape; seam gap
recorded in 0_trace.ts header: EDB-plane writes bypass SqlRunner via hand-
rolled execute$). FIRST PARITY NUMBER, ugly and now visible: ingest_corpus
over 251 rxjs .ts files = ~103s (~2.4 files/s) vs v5's 7,244 files/s; the
harness's per-file rt.rows() full-table read is superlinear and suspect, but
extract_ms is only ~21ms/file so engine-side cost dominates — the perf JSONL
now exists to decompose this. DECOMPOSED (overnight, 60-file pinned corpus,
DL_PERF_LOG): wall 4977ms = engine ticks 366ms (~6ms/file, 19 stmts/tick,
growing 4.4 -> 9.4ms as tables fill) + extract stream 438ms + subprocess
spawn ~0.6-1s + ~3s UNATTRIBUTED inside ingestFile (toFactLines, span_line
byte scan, diff reads — needs finer spans, wait for F7 merge since that
agent owns 4_ingest.ts). ALSO: DL_EXTRACT_BIN default is a DEBUG build in
the stale extract-golden-plan worktree (4_ingest.ts:93) — the banked
hardcoded-path soft spot is now a measured perf item; a release build +
in-tree path is the obvious first win. Endurance re-proven 3/3 on the
merged main tree (PORT=17311). TSV2 PHASE A LANDED (v6/tsv2: IGenProgram seam,
generic tickLoop via rxjs expand, 2 hand-carved gen files BYTE-IDENTICAL to
the prolog oracle incl a perturbed schedule, import gate green, 6/6 tests,
conformance 109; emitter-spec margins recorded in the agent report: keyed()
inert on raw arrivals vs live on edge heads, TEXT-collapse + LIKE compound
matching, one multiset-diff covers log+set, carryPending simplification
FINDING 3 in switch gen file). F7 CLOSED (merged, dl 83/83): root cause = multi-line sh output parsed
row-per-line instead of line-per-column, tag text through Number() = NaN,
typeof-guard passed NaN, bare NaN spliced into VALUES = "no such column:
NaN". Fixed: line-per-column parse when line count matches output-column
count, Number.isFinite rejection naming rel+column pre-SQL, execute$ errors
now carry the statement text (self-diagnosis gap closed), 4 fail-pre-fix
regression tests, failure-modes class 36 filed. Post-fix ghcacher marble:
resp/stars/full_name/change_log all land, stream alive.
PERF ARC RESULTS (overnight): attribution sub-spans landed
(read/extract_wall/fact/diff/commit); diff_ms was 77-81% of wall and O(n^2)
(unscoped SELECT through the correlated-subquery decode view, JS path
filter); FIXED via rowsForPath (WHERE on the interned path id against the
UNIQUE(path,...) index; promoted onto IDlRuntime by the coordinator, no
instanceof fallback). Receipts: diff_ms 3676 -> 16ms flat, 13.3 -> 74.2
files/s with the release extract bin (overall 2.4 -> 74.2 across the arc).
NEXT DOMINANT COST: commit_ms ~10.8ms/file (the store commit path), next
perf target, unassigned. v5 yardstick 7,244 files/s, distance now ~98x.
TSV2 RECONCILIATION: phase B merged (target-neutral plan term per the rust
directive, SQL-check 8/8 vs fixture expectations, 15/15 plunit); first
cross-run caught 4 emitted seam-shape misses (the clean typecheck was
vacuous until run-emitted.ts imported the drafts; gen_emitted/ now
quarantined from the type graph, drafts load via computed dynamic import,
package green). ROUNDS 2+3 LANDED, RECONCILIATION COMPLETE: emitted modules
on the A runtime are byte-identical to the prolog oracle on ALL THREE runs
(both fixtures + perturbed schedule), independently re-verified by the
coordinator. Round 2's finding: the tick-number dependency did not survive
the real seam (plan term converged to snapshot-diff deltas +
arrival-projected upserts). Round 3: compound columns render canonical term
text at read via CASE json_valid+json_type in lower.pl SQL (shared with the
future rust backend). gen_emitted back in the type graph. PHASE C SWEEP LANDED
(v6/prolog/compile/SCOREBOARD.md, regenerable via v6/tsv2/scripts/sweep.sh):
109 fixtures = 92 UNSUPPORTED (named constructs) / 9 IDENTICAL / 8 WRONG.
Ranked backlog: unmarked edge trigger 48 (needs real backlog-replay design,
not a quick widen) > comparison ops 12 > only+guard 9 > aggregates 9 >
arithmetic bind 5 > json destructure 4. All 8 WRONGs diagnosed: 5 = the
TEXT-only column model loses int/string distinction ("1" vs 1, structural);
1 = compound arrival text vs json1 match mismatch; 2 = rejection-semantics
fixtures with no comparable log. Sweep also fixed 4 real compiler bugs
(declared_refs union, multi-clause head DELETE wipe, @libsql number->REAL
integer corruption via bigint binds) and added a safety gate that caught 3
silently-miscompiled "identical" results (comparison/bind/head-arith now
refuse instead). Open: retention keep(count) not lowered AND invisible to
tick-log-only grading (needs final-state in the grade); 4 empty-schedule
fixtures pass vacuously. MORNING DESIGN CALLS: column typing (int columns
in storage vs TEXT-only), unmarked-trigger semantics, final-state grading
leg. PHASE D PARSER LANDED (2026-07-28, merge 10053236, coordinator re-ran
roundtrip.sh + conformance in worktree AND on merged tree): parse_dl.pl
DCG + print_dl.pl + dl_view/ (all 109 fixtures rendered as .dl text) +
SYNTAX.md + roundtrip.sh. G1 109/109 variant round-trip; G2 ghcacher.dl
7 decls/9 rules with 8 named gaps (host_decl/probe/query have no term-form
shape), conformance.dl 23/28 zero findings; G3 conformance untouched.
Central spelling call recorded in SYNTAX.md: bare identifier = variable,
atom constants single-quoted (supersedes dl.langium per the stopgap
ruling). Decl spelling: `rel name(cols) [log|set] [keep(..)] [key(..)]`.
NOT wired into compile_fixture yet — held behind the pending
latest/combine/zip + Key-decomposition words since they change spellings.
Hosts half of D still queued. ARCH.pl made current same morning (a6c1225a: tsv2 algorithm rows,
js_conformance_leg flipped done via the sweep, in-flight task rows;
go 7/7, atlas re-emitted).

**v5 BACKGROUND OPS (overnight 2026-07-27, user asleep)**: daemon swapped to
current binary (~/.cargo/bin/dl restored from target/release, was missing —
plist pointed at nothing while dl.old-1301 held the socket since Sunday);
launchd plist gained EnvironmentVariables PATH (homebrew+cargo) because every
sh effect exit-127'd under launchd's bare PATH — the doc-gen trigger then
fired and its output is committed (f76b7c10). Roots watched: sprefa (.dl/*.dl
rails + flow-interproc loaded), smashy, instant. CROSS-REPO IS LIVE:
~/orgs/.dl/{go-deps,xrepo-rev}.dl run against SPREFA_CONFIG=~/orgs/
all.config.toml (800 repos) settles in 3 ticks with real fan-in/rev-fan rows
(79 hubs). MORNING DECISION: the daemon runs the safe selfv5-only global
config; watching the orgs root persistently needs either a daemon-level
SPREFA_CONFIG (puts 800 repos under EVERY wildcard rail — the safe-default
comment warns against exactly this), a per-root config feature, or a cron
one-shot. Health also showed: sprefa root db regrew to 4.3GB (lazy-rel-tier
decision pending), 4 orphan roots incl one minted TODAY (class-14 rail may
have a gap — worth a look).
CLEANUP AUDIT LANDED (2026-07-28, opus, plans/2026-07-28-cleanup-audit-
findings.md): 24 tests audited, 11 sabotage probes, 0 removed; 7 mechanical
fixes merged (one-subscribe wildcard + zero-floor, import gate comment-blind
+ gen_emitted + direct-@libsql refusal, dead helper, 5 stale comments).
DEFECT WAVE FIXED 2026-07-28 PM (sonnet agent, 5 commits, coordinator
re-verified all suites + endurance): F2 commit() now REJECTS on tick-
pipeline fault (CommitSettlement union on reportsSubject, fail-pre-fix
5s-timeout test red->green ~2ms); F1 rowsForPath guarded by a rawSql
trace-seam test (sabotage receipt in test header: unscoped SELECT caught);
F4 bind channel + PerfTickLine.binds asserted; F8 fixture
log_stacks_within_tick_and_across_ticks added (corpus now 110; oracle-
verified; multisetDiff sabotage flips 2 fixtures red; sweep 31 compiled /
28 identical; roundtrip 110/110); labs/ DELETED from store (nothing
imported it; prolog_emit_bench.ts moved to src/bench/, swi_emit.sh
repointed; store 89->74 tests, last copy at 5d6f8fc5). dl now 92/92.
F3 CLOSED 2026-07-28 PM (merge 656694f1, coordinator re-verified 93/93 +
ratchet + endurance): BindConfig.scheduler (SchedulerLike, asyncScheduler
default, prod byte-identical), the only wall-clock rx source was
1_binds.ts interval(); both teardown tests rewritten on TestScheduler
asserting scheduler.actions.length (row equality could not discriminate a
leaked timer re-committing an identical bucket row — that WAS the false
positive), sabotage receipts red->green recorded in test headers; ~14s of
real test sleep removed. Two tests deliberately stay real-time (bucketFor
reads Date.now for VALUES; virtual firings inside one wall second would
collapse buckets). Endurance-as-gate still
ungated (open). COUNT-TEST LAW EXECUTED (cherry-pick 53762d1c -> main,
dl 96/96): EXPLAIN QUERY PLAN SEARCH-not-SCAN on rowsForPath's real
captured statement, only-requested-rows at 50 paths, and statements-per-
file exactly spineDeclsLocal.length flat across 5 vs 20 file corpora;
sabotage receipts in both test headers. PROCESS DEFECT, recurring: agent
worktrees are being cut from stale bases (three today: parser hook
failure, scheduler 244-behind, count-test 450-behind with NO v6/ — that
agent tunneled around a permission denial via git archive|tar to
materialize main's tree; disclosed, content read-only, branch history NOT
merged, test commit cherry-picked instead). Worktree-base staleness needs
a look before the next dispatch wave. CODEX LANE REOPENED (user,
2026-07-28 PM; pattern = claude-research/commands/codex-delegate.md,
OpenAI limits effectively free): first run LANDED (merge a48ed3f3,
gpt-5.6-luna, review-gated -- coordinator re-ran sweep 31/28,
conformance 110, roundtrip, plunit 17/17 on the branch): INTEGER columns
drop the json CASE wrapper via canonical_column_expr/3 int/text split;
generated SELECTs simplify to plain columns. Codex worktree removed,
branch deleted. Luna-ready brief queue: endurance-as-gate, lower/types.ts
I-prefix renames, v5 rails.dl descriptive names.
SPELLING MIGRATION WAVE 1 LANDED (merge f96c6229, codex luna, review-
gated: conformance 110, roundtrip ALL PASS, sweep 31/28 unchanged, grep
zero only(/departed( -- all re-run by coordinator): only() INVERTED into
bare-trigger + latest() sampling, departed -> finalize, combine/next
sugar, zip + unsubscribe/complete/subscribe/error reserved with named
refusals. 49 files. Key decomposition still split out (semantics arc).
CONSUMPTION+ARMS LAB LANDED (merge 445c345d, lab-death ee6bc71e, lab
commit 82bd12a8, verdict plans/2026-07-28-consumption-arms-verdict.md;
90 PASS re-run by coordinator, 7 rounds to fixpoint, 28 assertions):
consumption needs NO construct -- switch and queue are the SAME rel
under two key decls; all six observer words ground to shipped kernel
forms (subscribe/unsubscribe = next/finalize on the demand rel,
complete = finalize on the scope rel); every arm is row granularity.
Pacing: (b) one-per-drain-tick is the only spelling that implements a
queue -- (a) N-same-tick loses N-1 items at any keyed consumer and the
survivor is picked by term order of the ready view; (b)'s cost is the
drain cap becoming a queue-length cap (hard-fail at exactly 99/100
under error-at-cap). Crash-restart from the durable pending rel alone
yields ZERO ticks (durable rows do not make a firing durable --
SLOT-BOOT-OCCURRENCE, collides with the no-boot-replay endurance
goal). Error arm: reading (A) only (enum-variant destructure over a
Log envelope; second-channel refused on three grounds); on a KEYED
envelope an error arriving with a later ok row same-tick is replaced
before any arm sees it -- scheduler-batching-dependent observability,
the ruled collapse trace is the only record of the drop. Channel:
log + keyed cursor + min composes N-readers; keep(count(N)) is
tick-log BYTE-IDENTICAL to keep(all) while permanently stalling a
lagging reader (invisible to tick-log-only grading, same class as the
retention-grading gap). Desugar of signed edges: exact on plus,
inexpressible on minus (no retracting edge head). Retention slot
priced 4 ways, smallest honest = retention as an ordinary retracting
rule over the log (SLOT-RETENTION-SPELLING s1; cost = lifting
retract_from_log; rxjs has the same gap). 3 prospective fixture/5
terms graded by the real harness, recoverable at
82bd12a8:v6/prolog/labs/consumption_arms/fixtures.pl. AWAITING USER:
SLOT-QUEUE-PACING, SLOT-ARM-ARGUMENT, SLOT-ERROR-VARIANT-NAME,
SLOT-ERROR-TERMINALITY, SLOT-RETENTION-SPELLING, SLOT-COLLAPSE-CHANNEL,
SLOT-BOOT-OCCURRENCE.
EMITTER P0 LAB LANDED (merge 36977cb5, lab-death fcb47777, lab commit
53fa1f54, verdict plans/2026-07-28-emitter-p0-lab-verdict.md; user
unblocked sequencing before scale-bench landing; arc header =
plans/2026-07-28-incremental-sql-emitter-header.md + tempering): 4
statement families graded inline vs one-shared-helper on 4 fixtures,
12/12 tick-log byte identity, ZERO delta-side scans (EXPLAIN receipts
in the verdict). VERDICTS: semi-naive delta join MIXED (34 vs 7
lines), count-IVM support HELPER (42 vs 7), DISTINCT placement MIXED,
boundary-diff-from-delta-stream HELPER (16 vs 6, zero full-table
snapshots execute). Statement counts flat per tick, no arrival-row
loops. CRACKS: CURRENT_COMPILER_GATE (compiler refuses recursive/
departure/derived-edge fixtures; P0 emitted fixture-specific modules
graded through ScratchStore+TickFold); COUNT_CYCLE_RESEED (departure
coverage acyclic; cyclic P3 rides the retraction verdict's reseed);
CARTESIAN_CURRENT_SCAN (fork fixture has no equality predicate;
current side scans). P1-P4 now have proven shapes.
EMITTER P1 LANDED same sitting (merge on main after fcb47777, codex
sol no-commit flow, coordinator verified EVERYTHING on the branch:
sweep 31/28/0 per-fixture unchanged BOTH modes, conformance 110,
plunit 18/18, roundtrip, tsv2 6/6, import gate, tsgo clean):
incremental delta-join emitter is the DEFAULT for non-recursive level
rules; tick log computed FROM the delta stream (host_residency ruling
satisfied on the incremental path); SPREFA_TSV2_EMITTER_MODE=naive =
the snapshot referee; automatic naive fallback on retraction ticks,
negative bodies, edge+level mixed rels (P2/P3 scope). SCALE: s1/100k
177s -> 2.1s (84x), s2/100k 183s -> 1.1s (165x), ms/1k-arrivals FLAT
(s2: 15.8/11.7/11.1), s3/1k naive-OOM -> 6.5s under 512MB, statement
counts flat, all delta-side reads indexed SEARCH (p1-receipts.jsonl).
OBSERVABILITY-COST EXPERIMENT (coordinator scratch, DL_NO_OBS flag,
reverted, receipts in the merge message): s2/10k 1894 -> 199ms
(~=v1's 195ms), s2/100k 183s -> 10.2s naive-minus-snapshots (beats
v1's 17s) -- the WHOLE 10x-vs-v1 gap was boundary snapshot reads, v1
never paid the tick-log obligation (v1_scale_bench.ts emits no delta
log; asymmetry now noted here). P1 makes the obligation O(delta). PROCESS FINDING (codex lane): the codex sandbox cannot
write git metadata for coordinator-cut worktrees (.git/worktrees/*
is outside its writable roots) -- sol STOPPED AND REPORTED per the
dispatch law twice (ff-only, then git add); coordinator verified
(lab exit 0, 12/12 identical, conformance 110 on branch, tsc clean)
and committed the work itself. Older codex worktrees (unify, scale)
committed fine; difference unexplained, check before next codex
dispatch.
TYPES ROUND 2 LANDED (merge b47d3c00, codex SOL, lab deleted, last copy
20520177; verdict + plans/2026-07-28-types-as-rels-iteration-journal.md
are the record): fixpoint in 4 rounds (36->46->56->66 PASS + zero-finding
replay, re-run by coordinator). ENTITY and VALUE both first-class, NO
implicit default (missing policy = named failure -- user's not-sold
instinct upheld): value = content hash identity + dense-int mate +
immutable + support GC + set merge; entity = extrinsic id + mutable row
w/ immutable history + explicit checked retirement + keyed merge +
CYCLES PERMITTED (amends the round-1 cycle crack). Surrogate mate
VALIDATED: semantic hash and dense storage key are separate columns,
parent hashes consume child SEMANTIC hashes; resolves the dense-ints-vs-
content-ids ruling collision. Coexistence ranked hybrid > decl-word >
use-site (worked example in all three in the verdict). Support GC
complete ONLY on the value DAG; entity plane pays explicit retirement.
RULED 2026-07-29 (rulings.pl tail): decl_column_spelling =
colon_typed_ordered_columns (rel name(col: type, ...), source order
significant, Key(text) wrappers dead); enum_decl_in_rel = semicolon
variants in-decl; no_policy_suffix_words (set REMOVED, bare rel = set
table per engine.pl fallback, log = the only kind word, plane carried
by key(...) + id binds per the verdict's own optional-sugar note;
entity extras still need a future non-suffix spelling). Types arc has left design.
SPELLING WAVE 2 LANDED (merge aadede88, codex luna no-commit flow,
coordinator verified: conformance 112 (+2 fixtures), roundtrip G1
112/112, sweep 32/29/0 existing movement zero, plunit 18/18, tsv2
6/6, gate): 53 kind(Ref,set) entries deleted, `set` = named refusal
removed_word(set), colon types live (col_type(Ref, Column, Type)
term form, decl type is authority over C2a inference, contradiction
= decl_type_conflicts_witness), 2 new fixtures. ENUM ARC LANDED
(merge after 61817999, codex sol no-commit flow, coordinator verified:
conformance 115 (+3), roundtrip G1 115/115, sweep 34/31/0 existing
movement zero, plunit 21/21, tsv2 6/6, gate): semicolon variant decls
retained as sugar in term form (enum_decl/2), ONE shared expansion
expand_enum_program/2 (v6/prolog/0_enum_expand.pl) consumed by BOTH
the oracle engine and compile.pl -- variants become typed variant rels
(body_page/body_redirect) + derived body_tag view, reference columns
INTEGER, collision refusal named. SCOREBOARD.md noted stale (110-era)
by the agent, regen rides the next sweep-touching arc. P2+P3 HEADER SEEDED
(plans/2026-07-29-emitter-p2-p3-header.md): the 1M-competition entry
-- recursive strata across ticks, retraction as emitted support-count
SQL with the MANDATORY cycle guard, graded through the EXISTING
bench/engines rig against the PERF-REPORT.md standings (sqlite-count
class 443ms @960k is the target). Sequenced behind the enum arc.
RULED same night: rel_default_policy = value_unkeyed (bare rel = table =
replay subject); enum_variant_separator = prolog semicolon; enum storage
= N variant rels + derived tag view (lab (b), user walked through the
rust-single-table rejection). NAMED GAP from the channel thread:
retention driven by a derived rel (keep-until min(consumed.ordinal), the
Kafka low-watermark) is the ONE missing construct between log and
channel-with-N-readers; log+min-ordinal+consumed rows otherwise compose
it today. RETRACTION LAB LANDED (merge 2ef54e6e, lab-death a89acd3a,
lab commit 36980bf8, verdict plans/2026-07-28-sqlite-retraction-
verdict.md, 20/20 matrix re-run by coordinator against real sqlite3):
fk_cascade WRONG on shared children (kills child with live second
parent + dangling refs) and HARD-FAILS past sqlite trigger_depth 1000
(unraisable on this build; 1001-node chain = statement rejected);
support_count WRONG on cycles (counts never reach zero, both rows
survive a full release) and 9999 rounds/19s on a 10k chain;
recursive-CTE fixpoint reseed CORRECT on chain/shared/cycle/diamond
incl deferred-FK circular inserts, 8ms at 10k, no depth ceiling.
Crash-mid-cascade recovers both ways (ROLLBACK sim + real SIGKILL).
Confirms types-lab finding 6 (never emit FK cascade); reseed is the
retraction strategy going forward. REGISTRY LANDED (merge f414826f,
codex sol, review-gated -- coordinator re-ran conformance 110,
roundtrip ALL PASS, plunit 17/17, tsv2 6/6 + import gate, sweep
31/28/0 with SCOREBOARD byte-identical): surface/5 construct registry
(registry.pl) now drives analyze dispatch, refusal-by-absence
(unsupported_construct thrown for any functor without a live row),
parse/print body-word inventory, and a GENERATED SYNTAX.md construct
table (1_emit_registry_docs.pl). Bidirectional single-DCG stretch NOT
taken (variable-binding recovery + printer fidelity non-mechanical);
two files consult one table. One-row demo receipt (fake_reserved) in
task bmo2zn70a output. SCALE BENCH LANDED (merge 4dfac09c, codex luna two-phase incl
amendment-2 resume, review-gated -- coordinator re-ran conformance
110, tsv2 6/6, import gate on the branch; results v6/tsv2/SCALE.md,
brief plans/2026-07-28-codex-scale-bench-brief.md): 9-cell matrix
BOTH engines. HEADLINES: (1) tsv2 curve is superlinear as tables fill
(s1 ms/1k-arrivals 65 -> 196 -> 1771); (2) v1 evalProgramSql is ~10x
FASTER at every s2 size (17.1s vs 183.1s at 100k, 227MB vs 682MB RSS)
on the SAME recompute-per-tick class -- the gap is A-runtime overhead,
not algorithm; (3) tsv2 OOMs on s3 (2-atom combine cross join) even at
1k rows where v1 completes in 2.95s (1M-row result), v1 times out at
10k+ (shape is quadratic, but tsv2's memory blowup at 1k is its own
defect, unowned); (4) v1 s1 N/A-with-reason (no keyed-replace edge
semantics). Oracle cross-check byte-identical (sha 70c519e8). The
before-curve for the emitter arc now exists; P1 also owes the s3
memory answer. PROCESS: luna's first landing predated amendment 2;
resumed by session id per codex-delegate.md, model re-pinned. STORE-ADOPTION FINDINGS LANDED
(plans/2026-07-28-store-adoption-findings.md, sonnet, merged): PREMISE
CORRECTED -- the js store's cascade/reconcile is a generic liveness
propagator over (tag,id) keys + cx_dep edges, NO joins; the actual
derivation engine 3_runtime rides is lowerSql's DatalogEvaluator which
does DELETE-all + rebuild per tick and lodash differenceWith diffing,
the SAME naive shape as tsv2. The count-IVM-beat-DRed-4-5x receipt is
the RUST store only (engine.ts header says so itself). Consequence: no
js-store adoption win exists; the real tsv2 perf path is an incremental
join engine (rust store port or new strategy), to be motivated by the
scale bench curve. Prototype correctly declined with evidence.
TYPES-AS-RELS LAB LANDED (merge 7a416fac, 36 PASS, lab deleted, last copy
b58d1ece, verdict = plans/2026-07-28-types-as-rels-verdict.md): hypothesis
HOLDS on the value plane -- one construct (rel), struct = rel+set with
key(every content column), id = content_id() stdlib bind, enum = N variant
rels + DERIVED tag view, list = fixed-arity cons cells (amendment 1,
souffle made the same call), policy bundle = FOUR bits (identity/mutation/
lifetime/MERGE, amendment 2). THE CRACK: cycles -- content ids cannot
express cyclic graphs (parent id derives from child ids); cyclic needs
extrinsic keys where support counting stops being a complete collector.
DOMINATION DISSOLVES into support counting, complete because interned
graphs are DAGs by construction; graded: shared child survives (support
2->1), last release cascades 5 rows one tick; SQL ON DELETE CASCADE on the
same store deletes the shared child + leaves dangling refs = decisively
wrong + no rx lowering (finding 6) -- FK cascade must NOT be emitted.
Spellings priced (b) prolog functors > (c) plain rels > (a) json braces,
criteria visible, no fiat. Slots: OWNERSHIP-MARK = no mark on value plane;
ENUM-SHAPE = variant rels + derived tag; INTERN-SCOPE = per type;
JSON1-FATE = untyped json only never cache. Souffle verified (RecordTable
flyweight, monotonic = no GC precedent; bit-packing recollection REFUTED,
split is by field count). Top ambiguities: dense-ints-vs-content-ids
RULING CONFLICT (two standing rulings collide); tick log must print
VALUES not ids or migration/grading break; dictionary rels appear in
boundary deltas. ALL AWAITING USER RULINGS alongside the match-lab set. MATCH FRONTIER LAB LANDED 2026-07-28 PM (merge aeba1b72, 63 PASS, lab
deleted per protocol, last copy 5ba7b0c5, verdict =
plans/2026-07-28-match-frontier-lab-verdict.md): event axis HOLDS, four
cracks: Ta (DISSOLVES into pending rel, confirmed by tick-log diff both
ways — primitive Ta's log depends on an engine delivery choice, encoding
has no knob; rides the any-body-atom ruling), flagship transition rule
(C2 crack: loses N-1 of N intra-tick transitions, count depends on
scheduler batching), not() in +> arms (unstratified + arrival-order
dependent, silently), lifecycle arms over Log rels (statically dead,
retention prunes with no delta). C7 = REAL ENGINE DEFECT beyond design:
the Ti carry set is not durable in either implementation, crash loses
pending firings (endurance-law violation, unassigned). Slots: SPILL =
error-at-cap never spill; TA-MARK = no marker; NEST = not forced;
LEVEL-ARMS refuted-as-posed (engine already refuses; real restriction is
one-rel-one-rule-kind on heads); COMPLETE candidate = finalize(scope_row)
w/ groupBy duration selector; new open: SUGAR-SCOPE, UPDATE-ARM. Syntax
rec ordering: (1) Ta spells nothing; (2) SQL trigger family
inserted/deleted + OLD/NEW beats next/finalize (AFTER UPDATE gives both
in one body, kills the two-arm cut question); (3) drop mirrored -> (taken
twice, silent term-form absorption, conflicts q8 ruling), keep <-;
(4) +> optional sugar; (5) block word partition/groupBy over match;
(6) never => or | in term form. Rx directness 24 DIRECT / 1 vacuous /
7 ENCODED / 2 IMPOSSIBLE (Tn occurrences; incremental min/max over
retractable set). ALL AWAITING USER RULINGS.
gen-index.sh now excludes node_modules (INDEX.md was flip-flopping 1714 lines).
ARCH covers/2 rows for scopes.pl landed (departure_form fixture-covered,
uncovered 10 -> 9, map re-emitted). failure-modes class 35 filed (dangling dev
servers; stdin-watch rail proposed, awaiting word).

(v5 side: none. The 2026-07-19 AM wave is CONFIRMED LANDED on main, verified 2026-07-27:
src/eventlog.rs event trail + `dl daemon events`; `dl daemon health`
(src/cli/health.rs); class-14 rail (`hook::refuse_worktree_cold_check` +
tests/it/worktree_cold_check.rs); storage diet (a). `next` is 0 ahead / 244
behind main — nothing lives there. The 2026-07-18 wave landed in full earlier.
Detail for both: .agent/memories/sprefa-task-ledger.md. Receipts still live
from that wave: named_call_site is 61MB serving one join each,
inline-vs-keep = user call; .dl/rails.dl:62-64 still uses `p`/`l` and owes the
descriptive-name rename.))

### Blocked on user word
- [ ] **drop the orphaned `rel_port_of_reach` table + VACUUM** (one rewrite, not two): daemon stopped, `DROP VIEW IF EXISTS rel_port_of_reach_txt; DROP TABLE IF EXISTS rel_port_of_reach; VACUUM;` against `~/.local/state/sprefa/roots/fbabddda40d22347/db.sqlite`. Table 7.6MB + its PK autoindex 8.6MB = 15.5MB reclaimed; the deleted rule leaves the table behind.
- [ ] **rm the 3 overnight orphan roots** (~1.86GB, minted by agent-worktree pre-commit hooks before the class-14 rail existed): `cd ~/.local/state/sprefa/roots && rm -rf 5658fb5a59d0f252 c22f2b330d2dd1f7 ea3041acfc1af14c`. `dl daemon health` prints this exact line now.
- [ ] **lazy rel tier decisions** (plans/2026-07-19-lazy-rel-tier.md): syntax (`rel lazy foo(...)` vs `@lazy`), opt-in vs health-suggested, and whether demand-materialize-with-eviction is wanted at all or VIEW-only suffices (VIEW-only = zero new deps, zero policy code). Context: post-VACUUM the root db regrew 814 -> 877MB in hours with freelist ~0 (new pages, not churn); the 39x db/corpus ratio is the standing defect this decides.
- [ ] **filesize-rail ruling**: verify.sh exits 2 — 29 src files >500 lines are NOT in scripts/filesize-allow.txt (all already over budget at pushed main a3c09e3f, none crossed this session). Grandfather (allowlist + .dl/file-size.dl rows, shrink-only law) or schedule splits.
- [ ] **instant dom-match.dl rewrite** (user-side repo): drop pull/matches_latest/matches_body + both bucket columns onto `matches_resp(body) <- @async clock(5, _), matches() -> (body).` — caveat: matches_resp then accumulates distinct bodies unordered; keep a bound bucket if strict latest-wins matters.
- [ ] **worktree removal** (refreshed 2026-07-27, supersedes the 2026-07-19 row which undercounted by 40): reconcile pass found 42 worktrees. 34 are fully merged into main; all their uncommitted work is banked as 13 patches in archive/worktree-salvage-2026-07-27/ (README has per-patch inventory). `git worktree remove` was permission-blocked for the agent — the exact removal + merged-branch-deletion commands are in that README, run them. 8 unmerged trees stay alive (lsp-diags ahead 12, types, codex-intern, codex-qscip, g4-unify, refactor/file-splits ahead 7, vscode-flow-panel, extract-golden-plan RESOLVED: user merged it themselves (a85c9a70, 2026-07-28 PM) and checked out main; the session now rides main directly (cleanup/2026-07-27-reconcile is fully contained in main and stale). The DEBUG extract-bin default at 4_ingest.ts:93 now points at a merged tree, still a perf item).

### Next up (dispatchable, not started)
- [ ] **storage-diet 4a**: WITHOUT ROWID junctions; then A=1a dense dictionary ids; step 5 coordinate-composite elimination rides ref-spine. Direction 5 CLOSED 2026-07-19 (branch index-audit dc9b67b1: planner-honest demand filters in create_auto_indexes — PK-prefix on rowid tables, tiny-rel floor, constant-column; 771 -> 262 idx_, -117.7MB dbstat on the root snapshot; two policies measured-and-rejected with receipts: broad low-selectivity loses to value skew, PK-prefix on WITHOUT ROWID flips fixpoint join sides).
- [ ] **erase public no-daemon split** (user directive 2026-07-18): one server code path, `--no-daemon` internal-only; erases the two-db-worlds split. Big it-suite touch — schedule alone. Now also owns failure-modes class 23 (a one-shot positional under a daemon-served root silently returns the watched program set's results — `run_file_via_daemon` sends only `{"root"}`).
- [ ] **scheduler execution steps 1-2** (scope rows + readiness; shard = schedulable unit for every family, perf-fed costs, demand join as rows — d13dcf56). Write-volume budget lever lands here.
- [ ] **class 18 residuals**: ~~sg/ast_yaml internal ast-grep tree not shared with AstTreeCache~~ CLOSED 2026-07-19 (branch ast-tree-share: per-file SgRootCache embedded in AstTreeCache); ~~daemon-side req_id mid-tick cancellation~~ CLOSED 2026-07-19 (branch reqid-midtick 9ddf1280: run_job re-enters the causing request's reqid scope, cancel probe at component boundaries, abort-consistency test) — class 18 fully closed.

### Parked (wake on demand; plans exist)
- Auto-architect umbrella (docs/vision-auto-architect.md); decomposition + resource-scheduler children written, unexecuted.
- ~~Auto-refactor residuals~~ CLOSED 2026-07-18 (branch auto-refactor): audit found both "residuals" (brace-head rewrite, physical move + mod surgery) landed 2026-06-12 (#17, f859585e); this arc added the last gap, statement-level regroup when a brace leaf's rewrite exits its head. Audit table in plans/2026-05-31.
- vscode Wave 4; LSP thin client; turnkey query surface (`dl q`, verbs, MCP tools); measures top-K views; deck-graph sym-key migration.
- Change-cost friction inventory (plans/2026-07-10-change-cost-friction-inventory.md); ambient-config hermeticity top.
- Kimi trio prompts (reading-order/lib-taint/session-compile) — worktrees stale off old next; recut or delete.
- Low: 159-changed-paths mystery; tick_root pairing residual (c33ffc04).

### v6 STANDING PLAN (user-set 2026-07-25, execute IN ORDER, do not improvise past it)
1. ~~Restore green + commit~~ DONE (verified 2026-07-27: store 89/89, dl 74/74, both
   typechecks clean, `src/lib/rxjs.ts` orphan gone). Every green state gets a commit,
   standing. NOTE for item 2: the restored `sequence` helper still sits in
   engine.ts:115 with 2 call sites (:743, :744) — it is the first thing item 2 deletes.
2. ~~Undo rxjs over sync code~~ DONE 2026-07-27 PM (agent arc, merged): `sequence`,
   `run_then` (both copies), `execBatch`, `run$`, `inOrder` deleted; sequential run is
   `concat(...).pipe(toArray())` inline; rowsAffected flows through SqlRunner/batch/
   cascade/reconcile/TemporalStore/runAll; side-effect maps became `tap`; sync unwraps
   (`from(rows)->map->toArray`, rxjs `groupBy` over in-memory keys) are plain array
   code. Legitimate voids kept with reasons: `executeMultiple` (driver resolves
   nothing), rollback-path `catchError` swallows. Receipts: store 89/89, dl 74/74,
   both typechecks, ratchet 3, goal-endurance 3/3, statement counts unchanged.
3. ~~Single subscribe point~~ DONE 2026-07-27 late PM (agent arc, merged): ratchet
   reads 1, baseline lowered to 1. `serveDl(cfg): Observable<DlAppEvent>` in 6_http.ts
   IS the app, cold; main.ts's one `.subscribe` starts it. Program swap =
   `switchMap` on accepted loads only (bad program -> 400, running program survives);
   SSE clients are inners with `takeUntil(socket close)`; HostRunner lost
   start/dispose/Subscription for one cold `effects$` (boot replay under `defer`,
   semantics unchanged); `DlRuntime.commit()` now throws instead of hanging when the
   loop isn't running. Receipts: store 89/89, dl 74/74, ratchet 1, endurance 3/3,
   golden curl-session PASS, no Subscription fields. Honest residue: the
   `commits$`/`reportsSubject` Subject pair remains (not a collapse blocker, still
   the open item against the no-Subject-bridge corollary); `server.close`/`readBody`
   Promise wrappers remain (the Promise-above-the-seam arc); `tasks.d.ts:128` names
   `StartServer` in a past-tense M10 record (renamed to `ServeDl` in 0_types.ts).
   One golden flake in 1/10 runs under heavy parallel load, not reproducible,
   recorded in the agent report.
4. **Rxjs rule of engagement**: before writing ANY new rxjs, stop and ask the user
   first: is this making sense, is there a shorter/more direct way, fewer variables,
   fewer methods. No new operator chains land without that check.

### v6 primed queue (user, 2026-07-27 PM, unordered — "i want a lot of things")
- diags done + LSP hosted from TS (best-buy research first; note: v5 `dl --lsp
  --diag-db` boots NO engine and polls `diag_v5`, which 5_diag.ts already creates —
  the zero-code interim is pointing v5 at the v6 db).
- endurance goal: v6/dl/scripts/goal-endurance.sh IS the end-goal definition
  (kill -9 mid-delay, reboot, value lands exactly once). Phase 0 green; phase 1 =
  the pending-witness wedge + no boot replay of unanswered demand.
- snippets proving each v5 builtin rel's v6 behavior, ZERO new language features.
- bootstrap story: how the language owns its own utilities (swipl-to-C analogy);
  rust return eventually (souffle-of-rust + rx logic); formalizing the v8 event loop.
- self-diags on our own .pl files (pick up by pattern/extension/marker word).
- generic `--changed` concept (biome-style recent-change-lines gating) directable
  from dl; the old pre-commit hooks did this.
- graph-algo library in sprefa-store (user 2026-07-28: "very high source of
  non squared algos ... for complex graph algos either sqlite or ts if
  needed at runtime"): recursive-CTE and/or ts homes, build-vs-buy research
  first per standing law.
- lifecycle match arms (user 2026-07-28 design thread): every atom is a
  delta envelope (sign + = next, - = finalize, scope close = complete);
  bare atom = sugar for the + arm (the Result ?-unbox analogy); `match`
  reserved for subscription-time arms + envelope enums. Unruled, needs
  spelling + fixture work.
- `input/distinctUntil(shallowEquals|deepEquals)` on rels — mostly already physics
  here (R7 boundary diffing = distinctUntilChanged at every rel edge; set/keyed
  identical writes are zero-delta); the real residue is WHICH columns count as
  identity (= the Key/Q8 ruling) and digest-vs-value for structured blob columns
  (the content_hash pattern).

### v6 rulings RESOLVED 2026-07-27 late PM (three grunts; rulings.pl is the record)
- **salt_minting = content_addressed** ("one hunt"): shared in-flight effects, IVM
  support refcounting for free, freshness = explicit extra salt column. Consequence:
  **stale_fill_policy = not_applicable** — under content salts a fill is a cache
  update, never stale; no orphan rel, no fill tick-item, no per-instance identity.
- **effect_abort = best_effort_cancel_on_support_zero** ("rope arrow" + the
  invariant: "no arrow stop exist, is lie" — cancellation is cost optimization,
  never semantics; warn-paint at the abort site + debug line per attempt). Lowering
  owed: AbortSignal through HostDef.run + cancel map + pending-row delete (ARCH task
  effect_abort).
- **subscription_kernel = minimal_with_coverage_check_and_ghost_view**: zero stored
  rels, zero new phases; obligations = scope-coverage static check (ARCH task
  scope_cover_check, answers the zombie-scope break) + ghost forest diagnostic view
  (ARCH task ghost_forest_view). Shared DRed-depth hazard (recursive rels in scope
  cones = f(depth) statements vs n1_statement_budget) filed separately, owner
  unassigned.

### v6 REORIENTATION (user-set 2026-07-27 night): TSV2, prolog compiles TO TypeScript
NEW PRIMARY EFFORT (plans/2026-07-27-tsv2-compile-target-header.md): prolog owns
the whole compiler front (parse/AST/typecheck/lowering); it EMITS literal
TypeScript program files with the real SQLite statements and real rxjs chains
visible in the generated file. TypeScript keeps only (a) a hand-written static
runtime reusing the NAMED v6 symbols (SqlRunner, spine.ts fact plane, IVM
machinery, HostRunner lift, P0 tracing channels — class-34 law, import-gate
checked) and (b) the generated gen/*.ts programs. No AST/parser/lowering in TS
on this path. v6/dl stays untouched and running as the sibling; langium/
ast_bridge/lower are dead weight for tsv2 only. Grading = the item-9 tick-log
JSONL diffed byte-for-byte against the prolog oracle (the 109-fixture corpus is
the compiler test suite). Phases: A hand-carved target exemplar (2 scopes
fixtures) -> B prolog emitter matches it byte-identically -> C fixture sweep ->
D .dl DCG surface + hosts (ghcacher rides D). The stopping-point program list
below still defines DONE; programs land against the tsv2 target as it matures.

### v6 STOPPING POINT (user-set 2026-07-27 late PM): express the real programs
The milestone that ends this arc: the real programs written in the v6 surface and
graded, zero new constructs unless a program PROVES a gap (extraction-lab discipline):
1. **ghcacher** (poll -> fetch -> cache -> change_log carry; mode-lattice prog facts
   are the draft; content-addressed salts now ruled, so SWR spelling is open).
2. **diags for LSP** (diag rels -> diag_v5 view; the lsp-v5-bridge receipt is live).
3. **git pre-commit --changed** (biome-style recent-change-lines gating, generic and
   directable from dl).
4. **sprefa-extract run**: scan/scanwork, repo/rev extraction, lazy finding, lazy
   heads.
5. **auto-synced repo list**: HEAD the repo list itself (repo rows the system keeps
   synced; v5 repo-rev-scanning receipts research in flight).
6. **v5 bench parity target**: the v5 multirepo crawl benchmarks (grafana-class
   corpora) are the perf yardstick the v6 expressions must eventually meet.
7. **rtkq examples through sprefa-extract**: the redux-toolkit-query example corpus
   as an extraction+analysis target program.
8. **file watcher scaling, cross-platform preferably** (i:file-watching skill is the
   reference; watcher is a BIND per spine_residency, never kernel).
9. **standardized tick-log format**: the per-tick delta log serialized in ONE stable
   format (the marble record) so later runners (rust, python, ts) are graded by
   diffing logs against the oracle's log, never by embedding in the language. This
   is the json-rx cross-target agreement record made concrete.
Directive riding the milestone (rulings.pl spine_residency): the git/fs spine is
HOSTED IN THE LANGUAGE (stdlib rels + binds + salts over generic effect machinery),
never kernel; where the native concepts fail to host it intuitively, that is a
language finding, not a reason to special-case the spine.

### v6 rulings 2026-07-28 AM (rulings.pl is the record)
- **typed columns** (tsv2): int decls -> INTEGER storage; compounds stay
  inline-flat (punt); nested/reference storage model (struct-as-rel +
  surrogate id, the intern-dictionary pattern) BANKED as a future header.
- **unmarked edge triggers confirmed**: any-body-atom occurrence model (not
  whole-world), only() = opt-in restriction. C2 LANDED 2026-07-28 (merge
  c32dba53, coordinator re-ran the sweep to identical counts): typed columns
  (C2a, int/text inference per literal witness) + unmarked triggers (C2b) =
  scoreboard 9 identical/8 wrong/92 unsupported -> 27/0 wrong-diff/79
  (3 residual = pre-existing run_error/no_oracle fixtures). Named refusals
  banked: edge_trigger_is_derived (needs a tickLoop carry seam, ownership
  crossed so refused), edge_head_column_type_mismatch (2), edge_head_
  conflict_risk (1). Next unsupported buckets by size: edge_marked_with_
  extra_goal 21, comparison-in-level-body 14, aggregate_head 9, pre 8.
  C2 also fixed: both prolog test harnesses' hardcoded dead-worktree path,
  sweep.pl stale-output off-by-one, boot t=0 level closure over Initial.
- **clock_residency = world_fed_bind_not_construct** ("clock bind yes"):
  cadence enters as ordinary bind rows; SWR = rules over latest state joined
  with the clock rel; F2 gap dissolves at zero construct cost.
  LANDED 2026-07-28 (merge 378a39cf, sonnet agent): `1_binds.ts`
  BindDef/BindRunner = input twin of HostDef; activation by EDB rel-name
  match; commits$ merged beside effects$ in runProgram$ so program-swap
  switchMap kills bind timers; clock bind reads clock_period rows -> one
  interval per distinct period, bucket = floor(epoch_secs/period)
  (restart-stable); sprefa:bind tracing channel. Coordinator re-verified:
  typecheck clean, dl 90/90 (+4), ratchet 1, endurance PASS. Known limits
  (1_binds.ts header): clock_period config read once at subscribe (mid-run
  row needs reload to spin a new interval); no input-side dedupe cache
  (cadence has no witness — real asymmetry vs effect_cache). Agent side
  finding: bare fact `clock_period(2).` compiles to an IDB rule over a
  minted __lit_0 seed, not EDB. Follow-up open: ghcacher.dl gains real SWR
  via a clock_period row.
- **MERGED TO MAIN**: main fast-forwarded to the cleanup tip (aed4c155 ->
  9f8b6edc, 92 commits).

### v6.2.0 TAGGED (2026-07-29, local tag on e931191e, push = user)
P3 LANDED (merge e931191e, codex sol no-commit flow, coordinator
verified: sweep 34/31/0, conformance 115, roundtrip, plunit 28/28,
tsv2 6/6, gate, tsgo, store js 74/74): retraction = emitted
support-count SQL, guard per rule graph (plain count acyclic /
recursive-CTE reseed where cycles reachable), P2's three fallbacks
removed with fixture receipts, SIGKILL-mid-CTE recovery PASS.
COMPETITION ENTERED (PERF-REPORT.md standings, same input hashes):
tsv2-from-node DAG 60k 24.5ms / 240k 98.7ms / 960k 429.2ms BEATS rust
sqlite-count (31.3/105.1/443.0) at 23 stmts; CYC 960k 2756.5ms
CORRECT via reseed where rust bare count is wrong. Common memory
columns live in the shared CSV (host_peak_mb; sqlite_hw_mb =
N/A-with-reason, @libsql exposes no memory_highwater API; db_mb).
Honest cracks: support seeding not delta-proportional inside the
seed statement; multi-head recursive strata + multi-self-read
clauses = named unsupported; rust kernel_roots test skipped under
git law (writes .git/worktrees). FULL GATE ON THE TAGGED COMMIT:
conformance 115/0, sweep 34/31/0, roundtrip ALL PASS, plunit 28/28,
tsv2 6/6 + gate, dl 96/96, store 74/74, ratchet 1/1, endurance END
GOAL HOLDS. NEXT (user-agreed order): edge-off-derived carry seam,
then match block sugar.
HOSTS+EXTRACTION LAB LANDED (merge d7ac6926, lab-death 39f0733d, last
copy 2199456d, verdict plans/2026-07-29-hosts-extraction-verdict.md;
coordinator re-ran 41 PASS x2 + conformance 115/0 + roundtrip): term
inventory for the follow-up wiring arc = sh_decl/4 (EXPLICIT
input/output split, template edit never silently flips mode),
probe/4 (salt = plain column, identity vs witness digests split),
bind_decl/2 (decl authorizes, name links; zero-decl rel-name
activation REFUSED as the magic-rel hazard; `bind interval(...)`
selected, `clock` refused by the rx-name law), query/1 (whole rel
atom retained), ts_query/1 (12/12 tree-sitter query features mapped,
compiles to exact query text, unknown forms = named refusal),
sg_pattern/3 (own family; ts_query coercion refused,
slot_sg_metavariable_semantics). EXTRACTION FORK VERDICT: sg/ast/
tree-sitter/span take the HOST shape (EDB arrivals content-addressed
on (file_digest, query_digest), 1 invocation across N rules, feeds
edge rules as ordinary deltas); decode/json_each stay the
term-extract precedent. Ambiguities: A12 + A1 RESOLVED (push bind is
distinct from demand host; glob = host demand column), A4 + A14 stay
open with named slots. 5 fixture/5 candidates distilled for wiring.
DL6 DOOR LANDED (merge 9d096dd6, codex luna no-commit flow,
coordinator re-ran EVERYTHING in the worktree AND
conformance+roundtrip+text-door on merged main): compile_dl6/2 text
entry in compile.pl + compile_dl6.sh runner; text_door_receipt.sh =
34/34 byte-identical term-door vs text-door over the sweep's
compiled set; hand-written door-handwritten.dl6 (colon types + enum
+ latest + log) tick log byte-identical to the oracle; dl_view +
v6/dl/fixtures renamed to .dl6 (v5 .dl untouched); vscode grammar
dl6.tmLanguage.json GENERATED from registry.pl (emit_dl6_grammar/0),
dl6 language id contributed, extension compiles. Grades: conformance
115/0, roundtrip ALL PASS, sweep 34/31/0, plunit 28/28, tsv2 6/6 +
gate. Receipt scratch out/text-door gitignored, stripped from the
landing.
SQLITE UDF GRAFT LAB LANDED (merge 4bcf9aba, lab-death 9de6cddb, last
copy 9084850d, verdict plans/2026-07-29-sqlite-udf-graft-verdict.md;
coordinator re-ran PASS x5 stdout twice + conformance 115/0 +
roundtrip, which the agent had correctly fenced out of): v5 has 14
DISTINCT UDF names across 16 call sites (header's 16 was call sites);
usage: replace_re 38+21, regexp-as-=~ 78+13, split 34, lines 3+7 are
the hot ones, sym_intern used ZERO times in examples/.dl. DRIVER
REALITY: @libsql/client 0.17.4 has NO UDF registration API at all
(all four candidate method names undefined) -- the current TS seam
cannot register UDFs, period; better-sqlite3 .function() and sql.js
create_function both proven working, rust sidecar registration
proven, node-sqlite3 fails to load on node 24 arm64 (named slot).
GRAFT SHAPES per class: core SQL fuses where semantics match
(lower/upper/trim; parity 15-16/16, the misses are Unicode edge
rows); regex needs a function or sidecar (JS-compatible subset 15/15,
rust inline (?s) unparseable in JS); TS deopt proven delta-only (no
full-table scan receipt); emit-time for constants. Q4 ASSERTION SET
(P1 8 items / P2 5 / P3 8) handed to the running lift agent for
summary-time reconciliation. Q5: sprf_sym can feed content_id only
with type salt + canonicalization; dense intern mates stay
storage-only, never semantic identity (consistent with the types-r2
surrogate-mate ruling). Named slots: LIBSQL_UDF_API unresolved (the
eventual driver decision), INTERN_SIDE_EFFECT staging,
NODE_SQLITE3_ABI.
EXPRESSION+AGGREGATE LIFT LANDED (opus worktree agent, 6 commits,
merged; coordinator re-ran conformance 120/0, sweep BOTH modes 60
compiled/57 identical/0 wrong, plunit 40/40, tsv2 12/12, gate,
roundtrip on merged main): comparisons/arith/:= binds/concat/head
arithmetic fused into emitted SQL (WHERE + SELECT expressions);
aggregates count/sum = per-group accumulators, min/max = insert
delta-compare + GROUP-SCOPED delete recompute (EXPLAIN receipts:
SEARCH via PK, 1 of 5000 groups touched; sabotage receipts in test
headers); compiled 34 -> 60, identical 31 -> 57, conformance 115 ->
120 (+5 oracle-verified fixtures incl 2 pinning cross-type join).
Fail-first receipts red->green: TEXT-collapse (plunit
expression_miscompile_guards) + @libsql REAL bind corruption
(bootBind.test.ts -- BOTH harness boot loops bound params raw, the
one path skipping int->bigint). NEW final-state grading leg in the
sweep (closes the empty-schedule vacuity + makes keep(count)
non-lowering VISIBLE: final_wrong 3, all pre-existing). Q4
reconciliation caught a THIRD miscompile class: cross-type join
under affinity conversion ('1' vs 1 join = 1 row where oracle
derives none) -- now join_column_type_mismatch refusal. json
agg heads STAY refused: ordering reproducible in SQL but the
tick-log encoder renders prolog cons text ([|](4,...)), not json --
encoding gap, not order gap. Named cracks: edge bodies still refuse
comparisons/binds (no guard seam in the arrival-projection arm);
braces/list VALUES now refuse (were silently storing "null" /
{}(...) -- the phase-C "identical (vacuous)" braces row was a
miscompile); reconcile-frontier asymmetry commented in place.
POST-MERGE DEFECT, coordinator-found: text_door_receipt red on main
-- 20 lifted fixtures type via SCHEDULE literal witnesses, printed
.dl6 views carry no decls so the text door refuses
arith_operand_not_int; PLUS the receipt hardcodes =:= 34. Proven
fix: typed decls in the view text compile clean through the door
(hand receipt in chat). Fix = synthesize inferred colon-typed decls
into dl_view emission + dynamic receipt gate. Dispatched in the
3-lane blast.
3-LANE BLAST LANDED same sitting (all base 622dda3e, all re-verified
by coordinator in-worktree AND on merged main -- final merged
battery: conformance 120/0, roundtrip ALL PASS, TEXT_DOOR 62/62/0
exit 0, sweep both modes 62 compiled/59 identical/0 wrong (final 58
identical/3 pre-existing), plunit 47/47, tsv2 12/12 + gate, dl 96/96
(+1 soak skip under plain npm test), store 74/74, leak-soak 5
receipts green):
(1) TEXT-DOOR FIX (sonnet, merge after 394aacbe): print_dl
synthesizes colon-typed decls for WITNESSED undeclared EDB refs
(witness-less refs excluded -- freezing analyze's open(none) into
text broke 9 timeless_rail fixtures, found empirically); receipt
gate now dynamic (all term-door-compiled must pass text door),
two-stage grading replaces the silent skip reclassification;
sabotage receipt in header. Crack: witness check is ref-granular
not column-granular (noted in print_dl.pl header).
(2) EDGE-CARRY SEAM (codex sol, merge 78919aea):
edge_trigger_is_derived refusal REMOVED; derived edge triggers read
P1 frontier tables via the incremental dispatch; door program
byte-identical oracle-vs-tsv2 ticks 1/2/3 -- THE ENUM STATE MACHINE
IDIOM NOW COMPILES. Promoted edge_chain_hops_tick_per_stage +
demand_view_fires_its_consumer_once. Carry counts flat 100 vs 10k
rows, indexed frontier SEARCHes. Named crack: derived-trigger
programs use the incremental path even under
SPREFA_TSV2_EMITTER_MODE=naive (snapshot path has no delta stream).
MATCH BLOCK NOW UNBLOCKED per the user-agreed order.
(3) ENDURANCE GATE + NO-LEAK SOAK (codex luna, merge 9c1ffb4b):
green-all now includes endurance + leak-soak; leak-soak.sh = 20
swap/commit/SSE cycles then 5 receipts (handles/resources flat by
type via getActiveResourcesInfo, RSS bounded post-warmup +25%,
stmts-per-tick 10==10 via DL_PERF_LOG, SSE inner subs 0, bind
Timeout 1==1 across swaps); sabotage receipt in header; the three
law-debt soft spots (commits$/reportsSubject, server.close/readBody
wrappers, HostRunner boot replay) all CLEAR under soak.
ALSO FIXED on main pre-blast-merge (394aacbe): dl6-door rename had
broken 5 fixture paths in v6/dl tests/golden (dl suite was 89/96 on
main since the door merge, caught by the soak lane's baseline; the
door-arc merged-tree re-verify hadn't included the dl suite).
RULED (rulings.pl tail): json_ticklog_encoding = canonical_json_text
(json agg heads become emittable; oracle encoder change + one-time
regrade = the follow-up arc); udf_residency =
libsql_fuse_and_delta_deopt. STILL AWAITING USER (re-asked with
explanations): keyed-on-level-head refuse-vs-define, keep(count)
lowering choice.
RECURRING FOOTGUN, unowned: sweep regen DELETES non-fixture modules
from gen_emitted/ (door-handwritten.ts dropped THREE times this
sitting, restored each time); fix is sweep.ts leaving unknown files
alone or door-handwritten becoming a fixture.
MATCH + RULINGS LANE LANDED (merge 05f8ad29, codex sol no-commit
flow, coordinator re-ran in worktree AND merged main: conformance
126/0, sweep both modes 66 compiled/63 identical/0 wrong (final
63/2, both pre-existing runtime-error fixtures), plunit 54/54, tsv2
14/14, TEXT_DOOR 66/66/0, roundtrip ALL PASS, dl 96/96): match/2
block sugar via ONE shared expand_match_program
(v6/prolog/0_match_expand.pl, oracle + compiler both consult, the
enum-expansion precedent), arms expand to ordinary rules, enum
coverage checked (match_nonexhaustive refusal), sugar vs
hand-desugared tick logs byte-identical (sha b93e3028);
keyed_level_head refusal LIVE in oracle + compiler (fail-first
inert-accumulation receipt recorded; keyed edge head still
replaces); keep(count) LOWERED: one set-based DELETE...RETURNING
into the negative-delta path pre-P3, 12 statements flat at 3 vs 100
arrivals, retention_count_prunes_oldest final_wrong -> IDENTICAL.
+6 fixtures (4 compiled + 2 named refusals). Parser/printer/
registry/SYNTAX/tmLanguage all carry match blocks.
RULED same sitting (rulings.pl tail): keyed_level_head =
named_refusal; retention_count_lowering = retracting_rule_over_log
(both executed by this lane). ALSO RULED: json_ticklog_encoding =
canonical_json_text (regrade arc pending, unowned); udf_residency =
libsql_fuse_and_delta_deopt. USER DIRECTIVES 2026-07-29 late: CLI
("the bop") gates the 6.2.0 push -- registry.pl grows a cli command
table, emitter targets COMMANDER (required) on the TS side + clap
derive later, verbs serve/run/check/load/q, run+check boot the
server in-process (server-calls-itself, no daemon concept); spine
stays hosted per spine_residency with worktree as the UNMARKED
default source (no "WORK" atom -- pinned rev is the marked case);
kwargs partial application queued (task: body atoms may omit
columns = fresh wildcard, heads stay total; parse_dl
fill_free_slots :590 is the current exact-fill gate).

### Hands-on findings 2026-07-29 (coordinator wrote+ran a cold program; scratch fixture, receipts in chat)
- **keyed() on a level-rule head is SILENTLY INERT** (F8/retention-inert
  defect class): keyed(current/2,[1]) + `current(Id,Tag) <- door_tag(Id,Tag)`
  accumulated BOTH rows for key 1, no replace, no refusal (oracle
  engine.pl: decl_key consulted only in apply_edge_writes). Needs either
  a named refusal or defined replace semantics -- user call which.
- **edge_trigger_is_derived now blocks the flagship enum idiom**: the
  natural state machine `current(Id,Tag) <+ door_tag(Id,Tag)` runs in the
  oracle but REFUSES in tsv2 (banked C2 refusal), and the enum tag view is
  derived BY CONSTRUCTION, so enums + edge rules never compose in compiled
  programs. The banked refusal just got a lot more central; the fix is the
  tickLoop carry seam the C2 agent declined for ownership reasons.

### v6 still awaiting user word (small, none blocking the absorption arc)
- **Q8 residual**: confirm left-of-arrow = demand key on effect rels, `Key()` never
  appears there (the shipped TS reading; extraction lab's preference).
- **filesize rail + lazy-rel-tier + dom-match rewrite** (v5 side, unchanged).
- Tabling question CLOSED (plans/2026-07-27-tabling-verdict.md): SHIFTS SEMANTICS,
  hand-rolled fixpoint stays (the not_stratified guard IS semantics).
- **extraction ambiguities** A12 (from-world = nullary `->`?), A1 (glob residency),
  A4 (fence escape), A14 (comment_span bind). plans/2026-07-27-extraction-spellings.md.
- **Key(Type) vs `->`**: labs split three ways; present both files' arguments, no fiat.
  plans/2026-07-27-lab-consolidation.md bottom.
- Queued smaller: operators.pl models forkJoin as a level rule (correct only while
  inputs are unscoped — refixture when the sub forest absorbs); `scope_done`
  read-by-name violates the magic-rel ban (needs a decl); repeat's arrival-tick salt
  collides on two same-tick resubscribes; `until(F)` formula presentation in CLI output.

### Worktree dispatch law (user-set 2026-07-28, applies to every agent at every level)
- Every worktree agent's FIRST action: `git merge --ff-only <sha>` where the
  coordinator's prompt states the exact current main sha. If that fails, or the
  worktree is missing expected trees, STOP AND REPORT. Working around a blocked
  command through another mechanism (archive/tar, --no-verify, manual copying)
  is a defect, never a fix — a permission denial ends the approach, full stop.
- The coordinator verifies the agent's base sha in its first report and refuses
  work built on any other base (cherry-pick at most, never a history merge).
- **Lanes never spawn subagents** (user-set 2026-07-31, incident: cleanup lane
  spawned 8 unknown-model children, 0 commits, 0 receipts, killed by user). A
  worktree lane does its own edits sequentially; every dispatch prompt states
  this. Fan-out is the coordinator's call only, on luna/sonnet, never implicit.

### Lab protocol (user-set 2026-07-27, applies to every agent at every level)
- **Planner seeds the header first.** Every lab starts from a planner-written contract
  file: the predicates/checks the lab must implement, the questions it must grade, and
  named slots for ambiguities it may discover. No lab starts from a blank file.
- **Implementation agents run in worktrees** (Agent `isolation: "worktree"`), never in
  the main tree. Main-tree file ownership belongs to the coordinator only.
- **Labs die on landing.** In the same arc that a lab lands: durable output distills to
  its permanent home (conformance/fixtures, rulings.pl, plans/, ARCH.pl), the lab files
  are deleted, and the plan doc records the commit hash holding the last copy
  (`git show <hash>:<path>` recovers it). Git history is the archive.
- `v6/prolog/labs/` was deleted 2026-07-27 (last full copy at 2fff3f61) and stays
  deleted; a lab file surviving its landing commit is a defect, not a follow-up.

### Style notes for this repo
- **Comment budget law** (user-set 2026-07-31): comments state only constraints
  the code cannot show. No change-log narrative, no dates/arc/merge references,
  no restating the next line, no justification-to-reviewer, no essay headers
  (1-3 line module purpose max). Sabotage/fail-first receipts in TEST headers
  and scanner-backed @-waivers stay. History belongs in git/plans/ledger, never
  in source. Applies to every agent at every level; the 2026-07-31 cleanup lane
  is sweeping existing bloat.
- **Language vocabulary law** (user-set 2026-07-28): construct names and design
  discussion use ONLY rxjs, prolog, or SQL words. No invented terminology.
  Consequence under review: `only()` -> `latest()` (withLatestFrom), explicit
  `combine`/`zip`, `departed` -> rx-word candidate.
  ENFORCED 2026-07-29 on "support" (user: datalog-paper jargon, out):
  the concept is refCount, row-granular (count of derivations keeping a
  row alive; zero = teardown; cycles never reach zero = the Rc leak).
  Prose uses refCount NOW; identifier rename EXECUTED 2026-08-02 (opus
  lane t2-refcount, 25 files across prolog/store-js/store-rust/tsv2,
  byte-goldens untouched; ARCH row refcount_rename).
- **Every .dl snippet shown to the user carries its intended pure-rxjs
  lowering** (user-set 2026-07-28: "if u cant then we are not right"). A
  construct whose rx lowering cannot be written is a design defect.
- **Formerly-quadratic paths get COUNT tests** (user-set 2026-07-28): any
  path that was ever O(n^2) gets a test asserting the operation count/plan
  (statement counts, EXPLAIN QUERY PLAN SEARCH-not-SCAN), never end-state
  equality alone. Additive tests only; do not ravage working code for
  purity. Tracing/logging state in a single JSON file is acceptable.
- dl variable names are descriptive, never single-letter: `path`/`line`/`callee_name`, not `p`/`l`/`q`. Applies to every snippet in skills, examples, book, tests, and agent prompts; rename opportunistically when touching old files.
- N+1: never a per-row write. Collect the set, call `Db::insert_rows` once. The tick counter screams if you don't.
- No `provenance`/`substrate`/`load-bearing`/`regime` as prose or identifiers (use source/base/critical/mode).
- Sync tick engine: plural-API + collect-then-flush, NOT async DataLoader (the redux-out-of-hand trap).
- **Every new class declares its interface in the package's header `types.ts`** (user-set 2026-07-25): a
  class that ships without a contract in the header is an incomplete change, not a follow-up. The
  header declares each name exactly once, no `export type Foo = SomeFoo` aliases. v6 headers are
  `v6/sprefa-store/js/src/engine/types.ts`, `v6/sprefa-store/js/src/lower/types.ts`,
  `v6/dl/src/0_types.ts`. Currently uncovered: `tasks.ts` `Namespaced`/`Independent`/`Evidence`,
  `engine.ts` `AscendingIdQueue`. `Error` subclasses are exempt.
- **Important functions are interface-bound, never bare `export function`** (user-set 2026-07-25):
  TypeScript cannot conformance-check a standalone function against anything. A free
  `export function foo()` can drift from its documented signature and the compiler stays silent.
  So any function that matters gets bound to a header interface one of two ways:
  - namespace object, the default: `interface ISqlRunner { ... }` in the header,
    `export const SqlRunner: ISqlRunner = { ... }` in the module.
  - a class `implements` the interface, when there is real per-instance state or arg-object envy.
  The annotation is what buys the check. `satisfies` also checks and additionally keeps the
  literal's narrow inferred type; use it only when a caller needs that narrower type.
  Small leaf helpers that would be a `.map` callback or a plain method call in another language stay
  bare functions. This is the same exemption as the rxjs law.
- **Interfaces carry the `I` prefix** (user-set 2026-07-25): `IStore`, `IGraphNs`, `IDlRuntime`.
  The prefix is what lets the interface and its implementing object hold the same root word
  without an alias. `lower/types.ts` (`RelTable`, `Graph`, `Stratum`, `IDatalog`) is inconsistent
  and is the rename target, not the other way round.
- **Exactly ONE manual `.subscribe()` in the whole app, ever** (user-set 2026-07-25): React does
  not ask you to call `ReactDOM.render` three times. One terminal subscription at the bottom of
  `main.ts`; everything above it is cold and composed with `merge`/`concatMap`. A second
  `.subscribe()` anywhere is a design failure, not a style preference, because it means that
  branch of the graph is started imperatively and its lifetime is tracked by hand.
  Corollary: no `Subscription` field held on a class, and no `Subject` used as a request/response
  bridge (a method that pushes into one Subject and awaits a matching id on another is RPC wearing
  a stream costume, and it forces every caller back into `await`).
  Ratchet: TARGET REACHED 2026-07-27 (baseline = 1, never rises): the one site is
  `dl/src/main.ts` subscribing `serveDl(...)`. Remaining law debt, not ratchet debt:
  the `commits$`/`reportsSubject` Subject pair (3_runtime.ts) vs the no-Subject-bridge
  corollary, and the `server.close`/`readBody` Promise wrappers above the seam.
- **A type name must say what the thing is on first reading** (user-set 2026-07-25): no
  library-flavoured or abbreviation names that carry no content. `Rx` is the rejected example.
  If one interface needs a vague name it is usually two interfaces glued together; split it and
  both names get obvious.
- **Async becomes rxjs; sync stays sync** (user-set 2026-07-25, CORRECTING the earlier
  "make the whole code rxjs" instruction, which the user withdrew: "i should not have said
  make it all rxjs, just make the async into rxjs"):
  - `Promise`/`async`/`await` are banned above the single driver seam. That seam is
    `SqliteDb.execute`, wrapped exactly once in `SqlRunner` (`engine/sqlRunner.ts`).
  - Loops, branching, and list building over in-memory data stay **plain array code** and
    **return arrays**. `map`/`filter`/`flatMap`/`reduce`, not `from -> concatMap -> toArray`.
    A function that computes a `string[]` returns `string[]`.
  - The dividing line that works in practice (see `lower/lowerSql.ts`): SQL *building* is
    sync and returns statements; only *running* statements is an Observable. `runAll` is the
    single place a `string[]` becomes execution.
  - Symptom that the line was crossed: an Observable pipeline that ends by throwing its
    values away (`count()`, `toArray()` then ignore, `ignoreElements()`). That is sync work
    wearing an Observable. It also hides real values, which cost 8 redundant
    `SELECT count(*)` scans per conformance run before `rowsAffected` was let through.
  - `Observable<never>` is not used here. An effect emits one `void` when done and callers
    chain with `concatMap`; `concat` would union the effect's type into the value type.
  TRAP: `await someObservable` returns the observable without subscribing and TypeScript accepts it
  silently. Use `firstValueFrom`, or better, do not leave an `await` to convert.
- One rel = one rule kind: never head a rel with both a source rule (scan/match/ast/sg/json/cmd/comment) and a derived rule. `rebuild_derived` does a full `DELETE FROM rel` that would wipe the reconciled source rows. The engine now bails; split into two rels and union in a third derived rule. SAME hazard, separately guarded, for a **term-extract** rule (a `json`/`jsonp` body predicate over a bound string) headed together with a derived rule: `eval_extract_rules` fills the extract rows, then `rebuild_derived` (which runs after it so derived rules can read the extract output) drops them. Notably a term-extract rule cannot feed a `@next` carry directly for this reason — route it through its own rel first (the `pr_number -> change_log` split in gh-cache.dl). Engine bails as of the ghcacher-parity arc.
- Recompute guard: a fn that re-derives a relation/embedding FROM SCRATCH (a global op like `embed_graph`, run on a reactive rule) must early-out when its input is unchanged — a `load_rel_digest` digest skip (see `eval_node2vec_rule`, the scc/closure `ConditionCache.digest`) — or carry a `// @recompute unguarded: <reason>` waiver in its body. `examples/recompute-guard.dl --check` (exit 2) is the rail that enforces it; an unguarded recompute re-runs on every git-checkout re-tick under the daemon lock.

### ALPHA REORIENTATION (user-set 2026-07-29 night): dataflow frontier
v6 alpha finishes on CODE DATAFLOW ANALYSIS, not reactive wiring:
ghcacher demoted to phase-1-graded (live loop post-alpha); flagship =
a ported v5 dataflow program (flow-interproc / callgraph rail) graded
against v5's own output; alpha spine = extraction hosts + ingest perf
+ CLI + type pass. GOLDEN PLAN doc owed once the two opus reviews
land (language design review + v5-utility gap review, both in
flight); plan must carry their receipts. Labs run on OPUS ONLY
(user-set same night, supersedes the sonnet default for labs).

REL SPREADING LAB LANDED (merge 9220555c, lab-death c8eab9ed, last
copy 5bc5b6a4, verdict plans/2026-07-29-rel-spreading-verdict.md;
54 PASS x3 + conformance 126/0 re-run by coordinator): spread =
compile-time column splice via one expand module, spelling
`rel b(...a, extra: int)` SELECTED over `include a` (decisive:
splice POSITION changes the positional program, include cannot
state it) and term-form-only (no registry row, no text door).
Collisions refuse even when types agree (TS silent last-wins is the
graded negative); width subtyping refused by arity both directions;
planes/keys NEVER travel (inheriting keyed turns a loadable program
into keyed_level_head; inheriting keep is INVISIBLE to tick-log
grading -- the retention-grading gap class again); derived sources
refused (spread_source_not_declared -- the real line is GENERATED
vs INFERRED: enum variant rels legal after expansion, derived never);
row spread = fresh var per column, width from the TARGET atom's
declared arity. STRUCTURAL: spread rel arity stops being syntactic
(modifiers get computed arity, explicit arity checked); expansion
order FORCED enum -> decl spread -> row spread -> match. Cross-lang
receipts (real tsc/rustc/go) in the verdict. 6 named slots incl
slot_spread_marker_position + slot_spread_and_kwargs_overlap.
NOT WIRED -- design record only, wiring queues behind the alpha
priorities.

HOSTS WIRING PHASE 1 LANDED (merge 60631393, codex sol no-commit
flow, coordinator re-ran in worktree AND merged main: conformance
131/0, sweep both modes 69 compiled/66 identical/0 wrong (final
66/2 pre-existing), plunit 67/67, TEXT_DOOR 69/69/0, roundtrip ALL
PASS, dl 96/96): sh_decl/probe/bind_decl/query/ts_query wired end
to end -- oracle (1_host_expand.pl shared expansion, keyed
schedule-fed responses with late/duplicate replacement graded),
parse/print (`sh name(in)->(out) = template`, `? probe` + `@
salt(col: Val)` riders per SYNTAX.md:144, `bind name(cols).`,
top-level `? rel(args).`), registry, grammar, compiler (hostPlans/
bindPlans/queryPlans emitted as data; execution = named phase-2
unsupporteds). GHCACHER.DL6 PARSES GAP-FREE: G2 findings 8 -> 0
(19 decls, 9 rules, 2 queries -> 3 host plans + 2 query plans).
5 lab fixtures promoted: extraction_fork_callgraph/span_line +
native_ts_query_term identical BOTH MODES; both ghcacher fixtures
stop at the NAMED decode/2 compiler-lowering boundary (json-family
refusals = the known 16-fixture bucket). KWARGS PARTIAL LANDED on
this lane: omitted named-arg body columns = fresh anonymous vars,
partial heads refuse (partial_head), unknown/duplicate named args
keep named findings. New refusal: probe_mismatch(multiple_probes).
PHASE 2 (live host/interval execution in the tsv2 runtime) = the
named next arc, aligned with the alpha's runtime-bridging spine.
V5-UTILITY REVIEW LANDED (opus, read-only; full text in the task
output, relayed to user): headline = v6 is TWO DISJOINT RUNTIMES
and only tsv2 is graded (v6/dl still DELETE-all+rebuild via
lowerSql); distance table over the 9 programs (LSP diags = M and
closest, watcher ABSENT, CLI absent, scan() used in 105/129
examples with no v6 spelling); ingest 98x is the felt gap, the
retraction win is the unfelt one. Reviewer corrections by
coordinator: the 42-fixture "edge seam" claim is stale (seam
landed; the 42 are edge-BODY construct gaps: pre 12, latest 6,
negation 6, json destructure 6, now 5, finalize 2); SCOREBOARD +
justfile expected-count comments stale, refresh rides the next
sweep-touching arc.

UPDATE-ARM LAB LANDED (merge before 9788b655, lab-death 9788b655,
last copy be019a99, verdict plans/2026-07-29-update-arm-verdict.md;
19 PASS + conformance re-run by coordinator, main at 131/0 post
merge): zero-construct OLD/NEW arm HOLDS as
`changed(K,Old,New) <+ finalize(r(K,Old)), r(K,New)` -- EDGE arrow
mandatory, level spelling refused at load
(finalize_in_level_rule, refusal is right: level rules have no
occurrences). Fires replace-tick PLUS ONE (departure = next-tick
occurrence); needs NO keyed decl (property of the minus delta);
insert/delete arms silent by construction; delete IS separable
via not(current(...)). U4 same-tick v1->v2->v3 = ONE row, honest
ENDPOINT pair (v1,v3) -- settles match-frontier C2 as DEFINED
semantics (net transition per tick), residual trap = same-tick
from empty yields zero rows (firing is f(tick-start state)).
SUGAR-SCOPE ANSWERED: arm scope = trigger bindings + own body,
never siblings; loud-in-head (unbound_in_expression) but
SILENT-fresh-wildcard-in-body asymmetry = named slot. U5: finalize
over log rels silently dead (no refusal) -- SLOT-LOG-FINALIZE-
REFUSAL recommends load-time refusal, decidable. U7: compiled path
= edge_body_needs_finalize bucket (2 fixtures). Rx lowerings:
update arm = groupBy + distinctUntilChanged + pairwise (pairwise's
empty-first-emission IS the silent insert case); collapse = per-
tick fold BEFORE pairwise. 5 fixture/5 candidates distilled in the
verdict for promotion. Open slots: UPDATE-ARM-LEVEL-SPELLING (keep
refusal), DELETE-ARM-DISCRIMINATION, LOG-FINALIZE-REFUSAL,
ARM-SIBLING-WILDCARD, UPDATE-ARM-COMPILED.

LANGUAGE DESIGN REVIEW LANDED (opus, read-only, 16 cold-authored
programs through the text door; full text in the task output,
relayed): FOUND A LIVE BUG (A4, coordinator re-verified at HEAD):
world-fed key(1) rels DIVERGE oracle-vs-emitter since the
hosts-wiring commit (oracle keyed-replaces via absorb_set_arrival;
emitter still PK-over-all-columns + INSERT OR IGNORE; zero fixture
coverage kept 131/131 green; lower.pl headers state the now-false
premise). FIX LANE DISPATCHED (codex sol, brief plans/2026-07-29-
keyed-arrival-divergence-brief.md) also carrying B2 (three
silences -> load refusals: log-on-level-headed-rel A1, latest-in-
level-rule A3, pre-in-level-rule) + the gen_emitted sweep footgun.
Other top findings: A2 edge join cardinality = f(tick batching)
(3 batchings, 3 answers, no keyed rel involved -- latest() is the
knob and A12: the compiler REFUSES latest in edge bodies while
ACCEPTING the backlog-replay wrong program; B1 = lower latest as
the negation path minus NOT EXISTS); A5 typos/arity mismatches
compile clean, Name/Arity collision emits INVALID TS (duplicate
CREATE TABLE + TS1117), undeclared rel = legal EDB by the
edb_definition ruling (unpriced cost); A6 mid-tick level rows fire
edges while visible NOWHERE; A7 invalidation-as-log = permanent
poison (missing worked example: epoch column); A8 keep(count) is
per-rel never per-key (B6 wants a ruling); A9 the ruled collapse
LOGGING obligation is implemented NOWHERE; A11 count never 0;
B4 refusals print as swipl Unknown message with no file/line
(zero prolog:message//1 clauses -- "the worst part of the
cold-author experience"); B7 no float = no avg(); B8 vocabulary-law
violations BY THE LANGUAGE: pre/keep not rx-prolog-SQL words,
combine is an rx word with non-rx semantics (cross-join vs
combineLatest, receipt in review), finalize is stream-teardown in
rx not per-row retraction, now() binds tick not wall time and will
collide with clock binds. C3/C4/C6: SYNTAX.md hand-written half
stale (sh/probe/query rows missing from registry so the generated
table CANNOT cover them), refused constructs presented as surface,
two dead spellings still in grammar. D (defend): oracle-as-spec,
named refusals, predictable emitted SQL, one canonical parser,
the construct budget discipline.
GOLDEN PLAN WRITTEN (plans/2026-07-29-v6-alpha-golden-plan.md,
task closed): P0 correctness debts -> Phase 1 bridge the two
runtimes (hosts phase 2 + tsv2 engine under a served process) ->
Phase 2 extraction live (watcher bought, worktree-default
enumeration) -> Phase 3 dataflow flagship (edge-body arc latest
FIRST, flow-interproc port graded vs v5) -> Phase 4 CLI + LSP
milestone -> Phase 5 type pass + ingest perf. Alpha exit = 6
receipts listed in the plan. Out-of-alpha list recorded.

KEYED-DIVERGENCE FIX LANDED (merge after 69708b89, codex sol
no-commit flow, coordinator re-ran in worktree AND merged main:
conformance 135/0, sweep both modes 70 compiled/67 identical/0
wrong (final 67/2 pre-existing), plunit 70/70, TEXT_DOOR 70/70/0,
roundtrip ALL PASS, dl 96/96, main clean): review-A4 live bug
CLOSED -- keyed arrival targets emit PK over KEY columns + INSERT
OR REPLACE + old-row staging so the incremental minus delta
matches absorb_set_arrival; lower.pl false-premise headers
rewritten; fail-first fixture world_fed_keyed_arrival_replaces
(red both modes -> green). Review-B2 refusals LIVE in oracle AND
compiler with fail-first receipts: log_on_level_headed_rel,
latest_in_level_rule, pre_in_level_rule (3 refusal fixtures;
pre fixture moved generic->named reason). SWEEP FOOTGUN CLOSED:
sweep.sh removes only the fixture module it rewrites;
door-handwritten.ts survives (4 prior eatings, receipt = clean
git diff post-sweep). Corpus now 135; zero movement in the prior
131 buckets. ALPHA P0 = COMPLETE except SCOREBOARD/justfile
comment refresh (rides next sweep-touching arc) and the
json_ticklog regrade (ruled, unowned, S). NEXT PER GOLDEN PLAN:
Phase 1 runtime bridge (hosts phase 2 + tsv2 engine under a
served process).

JSON TICKLOG REGRADE LANDED (merge a680b54c, codex luna no-commit
flow, coordinator re-ran conformance 135/0 + sweep BOTH modes
70/67/0): ruling json_ticklog_encoding EXECUTED -- oracle ticklog.pl
+ final-state encoder render json values (lists, braces literals,
obj pairs) as canonical JSON text (sorted keys, no whitespace; NOW
PART OF THE CROSS-TARGET LOG CONTRACT); tsv2 runtime/ticklog.ts
canonicalizes identically at the sqlite seam. 244 oracle artifacts:
12 changed, 232 byte-identical. registry json agg comment cites the
ruling; emission refusal stays (later arc). TICK-MODEL.md committed
same sitting (9b50152b): the semiring/grading semantics (B/N/Z
rings, lifecycle = sign decomposition of the delta derivative, tick
grades on rule-graph edges), the five cross-plane refusals as its
hand-proven theorems, phase-5 checker spec; pointer at analyze.pl's
supported-subset gate. Golden plan gained: float/bool/null
recommended shapes (float=REAL+avg the one real hole; bool = row
presence / two-variant enum, never a column type; null NEVER,
Option = variants/absence) + rev shape (two variant hosts) +
extractor-is-fixed. STILL RUNNING: runtime bridge (opus), latest()
edge lowering (sol), doc-truth take 2 (luna, relaunched after a
correct STOP -- dead spellings reclassify to a legacy SYNTAX
section, never removed; G2 contract requires recognition).

LATEST() EDGE SAMPLING LANDED (merge 066bf3c3, codex sol no-commit
flow, coordinator re-ran in worktree AND merged main: conformance
135/0, sweep both modes 73 compiled/70 identical/0 wrong, plunit
74/74, TEXT_DOOR 73/73/0): the N->(0|1) coercion from TICK-MODEL.md
section 4 is real -- latest(RelAtom) in edge bodies reads the BASE
table (sampled, rx withLatestFrom; EXPLAIN receipts = PK/index
SEARCHes), bare atoms stay triggers, latest((conj)) refused. THE
INVERSION IS DEAD: marker_stops_backlog_replay (the correct
no-backlog program) now compiles identical while the unmarked twin
still replays per its own oracle. Movement: +3 identical
(marker/completion_lattice/identical_demand_dedups), 3 fixtures
moved to SHARPER named refusals the lowering uncovered
(trigger_arg_not_var(done) on scope_done_three_spellings;
edge_head_column_type_mismatch(demand_rev/2) on the two rev-pin
fixtures -- real pre-existing fixture issues, not papered). Sweep
footgun fix held: door-handwritten.ts survived the merged-main
sweep untouched. TICK-MODEL.md section 4 status row updated in the
same landing. Still running: runtime bridge (opus), doc-truth take
2 (luna).

DOC TRUTH WAVE LANDED (merge f2be3936, codex luna take 2 after a
correct STOP; coordinator resolved manifest conflict by regen):
registry rows for sh_decl/probe/bind_decl/query/ts_query/sg_pattern
(generated SYNTAX table now covers hosts constructs), hand half
updated (program/3, ghcacher zero findings), dead spellings
RECLASSIFIED to a legacy parsed-then-refused section (grammar
untouched, G2 contract), refused-vs-live split,
keep_on_non_log_rel refusal (engine+analyze, fail-first fixture --
theorem six), SCOREBOARD/justfile refreshed. Conformance 136/0.
RUNTIME BRIDGE PHASE 1 LANDED (merge 22607c08, opus worktree agent;
coordinator resolved justfile/SCOREBOARD conflicts, installed the
new tsv2 deps pino + @noble/hashes on main, re-ran EVERYTHING:
conformance 136/0, sweep 73/70/0, TEXT_DOOR 73/73/0, roundtrip ALL
PASS, plunit 75/75, tsv2 21/21, import gate 3 gen/8 runtime/7
serve, dl 96/96, serve-endurance HOLDS, serve-leak 20 swaps clean,
one-subscribe = 1 per app x2): FORK = PATH A WRAP with evidence
(adopt = rewrite: DlRuntime welded to interned storage plane +
langium Program type; wrap = 7 files v6/tsv2/serve/, v6/dl
byte-untouched). THE GRADED ENGINE IS SERVED: door-handwritten
over HTTP byte-identical to the oracle; live interval bind + live
sh host on VirtualTimeScheduler graded TOTAL via replay (world
rows read back per tick, oracle fed exactly those -- zero excluded
columns); drain-boundary difference MEASURED and stated (a
carrying tick with an empty queue drains: 3 ticks fed vs 4 served,
same deltas); endurance across 4 server generations (answered
witness exactly-once, unanswered at-least-once, stated); 7
sabotage receipts incl 2 the agent corrected about its own drafts
+ 1 named blind spot (db.close flips nothing -- libsql registers
no node handle). New recipes: text-door/one-subscribe in green;
serve-endurance/serve-leak-soak in green-all. dl6_oracle.pl = .dl6
text + JSON schedule -> oracle tick log (new grading door). Cracks
filed: served drain numbering differs when programs carry (not
fixable in general); replay grading blind to world-side sabotage;
dl6_oracle maps JSON strings to atoms (double-quoted literals vs
world columns need fixture terms); __host_witness is serve-owned
(zero-row answers need a cache row); bare top-level facts refused
(level_rule_no_positive_body) so seeds go over HTTP; door-
handwritten had been SILENTLY STALE (its latest-in-level compiled
module predated the refusal; repaired to plain level rule --
nothing gated checked-in gen modules, gap noted). PHASE 1 OF THE
GOLDEN PLAN IS COMPLETE. Next: phase 2 extraction live (fixed
extractor, watcher buy research, enumerate/enumerate_at hosts).

### NEW DIRECTIVE (user 2026-07-29 late, pre-save): memory soak + sqlite stats
Reactivity engine needs an interval-driven soak running a LOT of
contrived sqlite (massive assert/retract churn) proving node memory
pressure stays CONSTANT -- the serve-leak soak covers handles/RSS
over swaps, this one covers sustained write churn on one program.
Plus SQLITE STATS surfaced so behavior is characterizable across
impls: tsv2 first; READ what the v5 rust side already exposes in
src/db.rs (dbstat usage, sqlite3_status wrappers -- "a fuck load of
code in here") before building anything. ARCH task row: memory_soak
(unbuilt, after runtime_bridge_p1). Not dispatched yet.
ARCH.pl made current this save (12 new task rows: incremental_emitter
/expression_lift/hosts_wiring_p1/edge_carry_seam/match_block/
latest_edge_sample/runtime_bridge_p1/tick_model done; clock_check/
extraction_live_p2/memory_soak unbuilt; go ALL PASS).

### OVERNIGHT WAVE 2026-07-29 night (user asleep; forks recorded in ARCH.pl fork/5)
PROLOG ORG EVAL LANDED (codex sol, analysis-only, banked 2c43f931,
worktree+branch removed): plans/2026-07-29-prolog-org-review.md = 46
files/15,028 lines, emit_ts module collision (all-load fails), 18 body
walkers (9 direct-consolidate), 6 mirrored cross-plane checks + 2
engine-only holes (missing Log retention, aggregate edge head),
spread-order incompatibility (enum-first erases enum_decl/2 that match
coverage reads), ranked 10-row refactor table all test-first. ORG
REFACTOR ARC DISPATCHED on it (opus worktree, base 5a9bfdd8, order
R10+R6 -> R1 -> R2 -> R3 -> R4+R5 -> R9 -> R7+R8, STILL RUNNING).
WATCHER BUY RESEARCH LANDED (sonnet, merge 8b0b49a8, ARCH row done):
VERDICT @parcel/watcher first (native batch callback fits
engine.submit(IArrivalBatch) one-tick commits, ignore filter below JS,
prebuilds, 31M weekly dl) > node fs.watch (zero-dep fallback, IS
chokidar v4/v5's mac/win backend) > chokidar (ergonomics only) >
watchman (later backend upgrade, not the buy). 3 open residuals in doc.
Pick gets its fork/5 row at phase-2 dispatch.
MEMORY SOAK + SQLITE STATS LANDED (sonnet worktree, merged, coordinator
re-verified EVERYTHING incl 3 own soak runs): GET /stats on the served
engine (ServeStats: IServeStats; PRAGMA page_count/page_size/
freelist_count + ONE grouped dbstat statement via json_each bind,
forkJoin on the existing seam; dbstat PROVEN on @libsql 0.17.4) +
memory-soak.{sh,ts} churn driver (keyed replace + log keep(count) +
derived edge rel, 2500 ticks/100s short, TSV2_SOAK_LONG=1 overnight).
Receipts (coordinator's own runs): rss/heap/page-count flat, 37
stmts/tick flat, MEMORY SOAK HOLDS exit 0; sabotage keep_all RED exit
1 (page count 17->33 vs ceiling 19). STEP-0 FINDING: rust has NO
sqlite3_status wrappers -- v5's whole stats surface is db.rs rel_stats
dbstat sums + health.rs PRAGMAs; tsv2 mirrors exactly that, nothing
invented. justfile memory-soak recipe wired into green-all. BANKED
FINDING: tests/serveHelpers startServed retains every event by design
(fixture replay needs it) = false-positive growth at soak scale; soak
uses a private non-retaining subscribe (run-fixture precedent, outside
the one-subscribe scan paths).
PHASE 2 HEADER SEEDED (plans/2026-07-29-extraction-live-p2-header.md,
2976f893): contract + 4 named slots (watcher-event-shape,
enumerate-scope, extract-granularity, stale-demand); dispatch gated on
org-arc merge only now (watcher research + soak both down).
Coordinator task queue #1-#13 tracks the night; phase 3-5 rows seeded.
ORG REFACTOR ARC LANDED (opus worktree, 12 commits, merge 9186f1ad;
coordinator re-ran EVERYTHING in worktree AND merged main): ALL 10
review ranks. prolog-lint gate (ratcheted baseline 1, wired into
`just green` by coordinator) + emit_ts collision renamed;
0_body_walk.pl walk_body/3 consolidates 10 traversal sites;
0_program_check.pl = 6 mirrored cross-plane checks one impl + BOTH
engine-only holes closed compiler-side (missing Log retention,
aggregate edge head; fail-first receipts) -- log_without_retention's
checked-in gen module (a program the oracle rejects) DELETED, the
gen-modules-staleness gap's first real casualty; 1_expansion.pl
declared phase order + enum metadata context (analyzer double-
expansion gone, spread = placeholder rows); expression operator
inventory (5 local lists dead); R9 shared decl queries; R7 14 dead
exports removed of 44 classified; R8 private call sites 10 -> 1;
R4 oracle aggregates on registry axis (oracle stays wider than
compiler). Battery: conformance 137/0, plunit 124/124, TEXT_DOOR
72/72/0, sweep both modes 72/70/0-wrong, dl 96/96, store 74/74,
green exit 0, memory-soak HOLDS on merged main. Justfile expect
comments refreshed (were 3 landings stale). 4 findings banked
test-pinned (ARCH row org_banked_findings). Disclosed deviation:
3 module qualifiers in sprefa-store/bench/v1-scale-gen.pl (rename
fallout, output byte-identical). Journal:
plans/2026-07-29-prolog-org-refactor-journal.md. Agent note worth
keeping: findall/3 copies its template, bit twice with far-away
failure messages.
PHASE 2 EXTRACTION LIVE LANDED (opus worktree, 6 commits, merged;
coordinator re-ran EVERYTHING in worktree AND merged main -- green
exit 0, conformance 137/0, tsv2 28/0/1skip, sweep both modes 72/70/0,
EXTRACTION LIVE HOLDS 8/8, ENUMERATE HOSTS HOLD): watch bind on node
fs.watch behind an injectable IWatchSource seam (ZERO new deps per
fork watcher_first_impl; @parcel/watcher = one-adapter swap on user
word). SLOT DECISIONS (agent, reasoned, ARCH task row carries them):
event vocabulary never crosses the seam -- watch(glob, path, digest)
rows with arrival SIGN (rename = -old +new one batch; atomic save =
digest change; identical bytes = zero delta; no kind column, no
null); bufferTime(100ms) NOT debounce (git checkout ~20 ticks,
bounded both ways); enumerate = git ls-files pathspec (tracked-only,
node_modules NEVER WALKED, 972-on-disk/0-in-answer receipt);
MEASURED: git ls-tree accepts no glob pathspec, enumerate_at = 
ls-files --with-tree + rev-parse blob oids; one demand row per
(path,digest), declared outputs = named projection over extractor
JSONL. LIVE DEFECT FOUND+FIXED (fail-first): __host_response_* keyed
on witness digest ALONE silently lost all but the last row of every
multi-row host answer (every extractor is multi-row -- would have
blocked the whole feed); now ordinal:int, key (witness,ordinal),
oracle+emitter same-arc per the A4 lesson. EXIT RECEIPT
extraction-live.sh: real edit -> extraction -> finding, atomic save,
content-addressed zero-tick, delete retraction, restart zero
re-extraction, kill -9 mid-extraction exactly-once. justfile grew
extraction-live + enumerate recipes (green-all), tsv2-test comment
28/1. LEDGER-WORTHY EDGE the arc hit: dl6_oracle door maps JSON
schedule strings to ATOMS while double-quoted rule literals are
prolog STRINGS -- oracle derives nothing where emitted SQL derives
correctly; fixtures spell literals single-quoted (SYNTAX ruling
agrees); real divergence surface for cold authors. Also:
4_ingest.ts:93 DEBUG default unchanged, but extraction-live.sh now
exercises the correct resolution order (DL_EXTRACT_BIN -> in-tree
release -> build), the shape 4_ingest should adopt. Watcher
restart+delete retraction gap + one-shape-per-host-decl limits named
in ARCH row. PHASE 2 OF THE GOLDEN PLAN IS COMPLETE.
PHASE 3 EDGE-BODY ARC LANDED (opus worktree, 4 commits, merged;
coordinator re-verified: green exit 0 on merged main, sweep both
modes 82 compiled/80 identical/0 wrong, TEXT_DOOR 82/82/0, plunit
134/134, conformance 137/0): negation/comparisons/binds guard seam,
now/1 = emitted tick counter, edge heads inherit column types from
feeding bodies (5 rev-pin/diag fixtures flipped). THREE HONEST
STOPS, all receipted: (1) pre-in-edge REFUSED -- the dispatch
premise (pre = sampled read) was MEASURED WRONG; pre is a chained
mid-tick read through ordered occurrences (pre-as-sampled projects
[1,1] where oracle pins 2, SCOREBOARD.md receipt); needs an ordered
occurrence loop = new execution shape, ARCH row pre_occurrence_loop.
(2) finalize-in-edge = runtime seam owed (frontier staging drops
sign=-1; signed departure frontier for listened_departure_refs).
(3) json destructure blocked BELOW the arm: compound-arrival term
text vs json1 tagged form -- the encoding decision precedes any
decode/2 lowering, ARCH row decode_arc, user-level call. NEW REFUSAL
THAT GATES THE FLAGSHIP: edge_body_joins_arrival_fed_level (emitted
mid-tick level plane is insert-only vs oracle's freeze-after-
arrivals-before-edges; clock_rel_join_storms 3 rows vs oracle 1).
TICK PHASE ALIGNMENT ARC DISPATCHED on exactly that + the signed
frontier (ARCH task tick_phase_alignment, fork tick_alignment_tier).
Fallout fixes in the landing: seeded_refs Initial-only silent row
drop (final-state leg caught it), print_dl col_type synthesis.
ALSO: `just arch` cwd bug fixed by coordinator (covers_endpoints_
ground read a repo-root-relative path; now resolves via
prolog_load_context, passes from any cwd). TWO FLAKE CLASSES seen
tonight, both pre-existing: store golden.test 1-in-N under parallel
load (ledger'd before), one node segfault in a sweep run (clean on
rerun, experimental transform-types under load suspected).
TICK PHASE ALIGNMENT LANDED (opus worktree, 2 commits, merged;
coordinator re-verified worktree AND merged main green exit 0):
THE FLAGSHIP BLOCKER IS DOWN. Mid-tick level plane now freezes
where engine.pl freezes it (recomputeLevelsBeforeEdges shares the
emitter's own 5 supportSql; naive referee recomputes before AND
after edges); edge_body_joins_arrival_fed_level REMOVED,
clock_rel_join_storms byte-identical both modes (was 3-vs-1 rows).
Departure frontier = separate __departure_frontier_<rel> TEMP table
per listened rel (sign-column rejected with the all-83-modules-
byte-identical receipt); finalize-in-edge flipped; departures
counted in carryPending. Sweep 82/80 -> 85/83/0-wrong, TEXT_DOOR
85/85/0, plunit 137/137. HOLE FORCED SHUT en route: flipping the
finalize registry row deleted the generic refused-goal catch and
the compiler ACCEPTED finalize-in-level; finalize_in_level_rule
restored shared-side, drift became an agreement test. C7 INHERITED
not closed (ARCH row c7_durable_carry): staged departures die with
the connection like the rest of the carry set; durable-carry = own
arc. door-handwritten went stale a SECOND time (periods->literals),
coordinator regen'd + filed ARCH row gen_staleness_gate. Justfile
expect comments refreshed again (85/85/0, 85/83, 137/137, 44/1).
REMAINING edge-body buckets: pre 13 (pre_occurrence_loop arc),
json/decode 15 (decode_arc, user-level encoding call first).
FLAGSHIP DISPATCHED per fork flagship_pick: callgraph rail first
proves the v5-output-vs-v6 grading rig, flow-interproc rides the
same rig.
FLAGSHIP STEP 1 LANDED (opus worktree, 2 commits, merged;
coordinator RE-RAN THE RIG ITSELF exit 0 same table + green +
conformance 139 + sweep 87/85/0): THE ALPHA'S HEADLINE RECEIPT
EXISTS. examples/callgraph-ast.dl byte-unmodified vs its v6 port
over a pinned 13-file rust corpus (scratch one-commit repo as
shared cwd so paths match; v5 leg DL_STATE_DIR-isolated, daemon
untouched). CLASSIFICATION: def 57/67, call 36/217, calls 143/574,
unused 39/23 -- EVERY diff row bucket (a) extraction-input, 0
expression gaps, 0 defects. v6 strict superset on monotone rels
(tree-sitter sees trait signatures, method/path/struct-literal
calls that v5's bare-identifier ast query cannot), unused inverts
as anti-monotone must. calls/unused proven by RULE FIDELITY (v5's
rule bodies run against EACH engine's own inputs -- this leg
defeated a wrong classifier draft that sabotage caught). Glob
semantics divergence pinned by assertion (v5 globset vs git
pathspec). 2 fixtures promoted; `just flagship` in green-all.
NAMED GAPS: probe-output guard refusal misnamed + unlocated (ARCH
probe_output_guard, B4 in the wild); byte spans cannot enter
programs (decode_arc dependency); NEW UNOWNED DEFECT extra drain
tick after refCount re-assertion (ARCH extra_drain_tick).
flow-interproc = BLOCKED (ARCH row): rides SCIP-resolved builtins
the phase-1 extractor CLI does not emit -- unblocking is a USER
call (expose the resolve pass) since the extractor is fixed.
reaches/closure not graded (cyclic; graph-algo queue item).
CLI "THE BOP" LANDED (sonnet worktree; merge + coordinator commit
6dcd02c9 -- PROCESS DEVIATION: agent reported done with the whole
tree UNCOMMITTED; coordinator reviewed file-by-file, ran every
receipt itself, committed on the branch; exactly the sonnet failure
mode the user warned about, caught by the dispatch-law base check):
registry cli_command/3 -> commander bop.ts, serve/run/check/load/q,
run+check boot serveTsv2 IN-PROCESS, no daemon; exit contract
verified by coordinator's own runs (clean 0, broken/missing 1,
ghcacher named refusal 2); 12 tests + inventory parity, tsv2 suite
56/0/1skip; commander = the one dep (user-required); one-subscribe
1/1 both apps. bop_check.pl documents the swipl halt-inside-catch
trap. tmLanguage regen fixed pre-existing finalize-still-reserved
staleness. justfile: bop-test recipe. Post-merge trip: main
node_modules lacked commander until pnpm install (9 bop tests red,
then all green; golden flake hit again on the same run, 74/74
isolated -- ARCH row golden_flake_hunt filed). THE 6.2.0 TAG GATE
("the bop") IS SATISFIED -- push + tag = user call. LSP milestone
= the remaining phase-4 half.
LSP DIAGS MILESTONE LANDED (sonnet worktree, committed 77b5bbce
after TWO coordinator continue-nudges -- the agent kept ending its
turn to wait on background builds; work itself clean; coordinator
re-ran the receipt LSP DIAGS HOLDS exit 0): ZERO NEW LSP CODE.
diag-rail.dl6 declares rel diag_v5 in v5's exact 9-column shape;
tsv2 names tables by bare rel name (lower.pl table_name) so the
program's own rel IS the table src/lsp.rs:545 selects -- bridge
fully in-language, no serve projection, no view. Receipt: no-eval +
unused-def rails over the live watcher/extraction feed, engine-side
appear+retract AND the real v5 dl --lsp --diag-db over real stdio
JSON-RPC receiving publishDiagnostics both directions (editor
rendering + clean shutdown explicitly NOT proven). Line numbers
HONESTLY ZERO until decode_arc lands spans. Sabotage receipt:
renaming a diag_v5 column passed every engine-side phase (curl
reads positionally) and went red only at the real LSP client --
the v5 leg is the discriminating check. TWO REAL FINDINGS -> ARCH
rows: emitter_groupby_literal (bare integer-literal head columns
read as POSITIONAL refs in the support GROUP BY; 0+0 workaround in
the rail), v5_lsp_exit_hang (dl --lsp answers shutdown then hangs
on exit+EOF, reproduced standalone, disclosed not patched).
justfile: lsp-diags recipe in green-all. PHASE 4 OF THE GOLDEN
PLAN IS COMPLETE (CLI + LSP). Golden-plan alpha spine now P0-P4
DONE; phase 5 (type pass float/REAL+avg, clock checker, ingest
commit_ms) is the remaining leg, all rows priced in ARCH.
MORNING SESSION 2026-07-29 (user awake, ruling round + dispatches):
USER DIRECTIVE: problems are "turbo mid", find the SMALLEST CORRECT
solution, standing for the whole open list. RULED compound_storage =
struct_as_rows (rulings.pl; user "lol d"): struct value = rel row +
content id ref, inline blob dead, decode dissolves into joins. BOTH
sharp edges worked in plans/2026-07-29-struct-as-rows-header.md:
(1) tick log prints VALUES never ids via memoized rendered_text
canonical JSON written once at intern time (DAG = children first,
parent render = one concat; boundary read = one join); (2)
dictionaries are boundary-invisible storage plane (frontier-TEMP
class; __host_* rels differ: both sides derive those). Lab receipts
adopted not relitigated (rendered_text_stable_under_both_policies,
support-GC complete on the value DAG, FK CASCADE stays banned,
json1 = untyped only, per-column ref(Type) coexistence).
STRUCT-AS-ROWS ARC DISPATCHED (opus worktree, base 72d4d753, IN
FLIGHT): unlocks 20 json fixtures + byte spans + LSP line numbers.
decode_arc CLOSED as superseded.
EXTRACT RESOLVE FLAG: user WAIVED the extractor-fixed directive for
exactly this ("sic codex terra on this once u scout to make goal");
coordinator scouted first: resolve is library-tested
(tests/0_prolog.rs:85 Resolve::<CallF> + def index + ProjectCx
recipe) and the CLI is phase-1 BY DESIGN (extract.rs:176) -- the
missing test was unwritable because the bin never claimed phase 2;
the real gap = nothing asserts bin-vs-lib capability parity. Brief
plans/2026-07-29-extract-resolve-flag-brief.md (flat top-level JSONL
fields, project-mode entry, CLI golden test pins the new contract,
zero deps, no-commit flow). DISPATCHED codex terra (header echo
verified, IN FLIGHT, worktree ../sprefa-codex-extresolve, branch
codex/extract-resolve, base 0264d2b1). flow_interproc_port now
blocks on this lane; closure spelling rides the port arc.
NIGHT CLOSE-OUT: first merged-main green-all tripped on a STALE
target/release/dl (Jul 20 build; receipt scripts build only if
missing -- binary flavor of the staleness class, noted on the
gen_staleness_gate row); coordinator rebuilt from current source.
FINAL RECEIPT, merged main, coordinator's own run: `just green-all`
EXIT 0 -- END GOAL HOLDS, ENDURANCE HOLDS, MEMORY SOAK HOLDS,
EXTRACTION LIVE HOLDS, ENUMERATE HOLD, FLAGSHIP GRADED
0-unclassified, LSP DIAGS HOLDS, sweep 87/85/0-wrong. Overnight
totals: 7 arcs landed+merged (org refactor 10 ranks, memory soak,
watcher research, phase 2 extraction live, phase 3 edge-body +
tick alignment + flagship, phase 4 bop + LSP), conformance
136 -> 139, sweep identical 70 -> 85, plunit 75 -> 137, tsv2 tests
21 -> 56, every fork/decision in ARCH fork/5, every open defect a
priced ARCH row. AWAITING USER: push main + v6.2.0 tag (bop gate
satisfied); phase 5 go-ahead; the standing v5 pile. RESOLVED morning
2026-07-29: decode/encoding superseded by compound_storage =
struct_as_rows; flow-interproc extractor scope waived -> terra
--resolve lane LANDED (merge 17778bbb + 0_prolog ledger refresh
c26b4e0e, worktree+branch removed, flow_interproc_port unblocked);
watcher dep RULED fs_watch_until_bench_regression (rulings.pl: TS
host binding is temporary, rust one day; @parcel/watcher = the
one-adapter swap taken only on a measured bench regression).

### FLASH-VS-OPUS LANE WAVE MERGED (2026-08-02, branch codex/rel-ref-file-span-lab, UNPUSHED)
(Sessions 2026-07-30..08-01 are ledgered in ARCH.pl task rows + chat_log,
not here; this section resumes at the lane wave.) Head-to-head:
opencode/deepseek-flash4-0731 onboarded as a delegation lane
(~/.config/opencode/opencode.json pins deepinfra allow_fallbacks:false +
reasoning high); 5 queued tasks x {flash, opus} = 10 worktrees at
~/projects/sprefa-lanes/<t1..t5>/<model>, identical briefs, no commits,
REPORT.md contract. Scoreboard
plans/2026-08-02-flash-vs-opus-lane-report.md: flash = excellent
brief-follower, weak skeptic -- won nothing, usable on 3/5 mechanical
tasks ($0.78 for 5 lanes, ~$1.05 the night); opus falsified 3
coordinator-fed claims, headline = the DEAF WATCHER non-bug (coordinator
filed an engine defect TWICE; opus t3: bop run self-exits after
BOP_RUN_IDLE_MS=2000 idle, bop.ts:165, and the receipt scripts' 3s polls
were talking to a dead process; surviving real finding = cold-boot host
spawn ~1s/subprocess). MERGED next sitting, 7 commits, zero conflicts,
coordinator re-ran the full battery on the merged tree: conformance
281/0, plunit 276/276, TEXT_DOOR 196/196/0, sweep regen zero byte drift
(gen_emitted/SCOREBOARD/manifest), tsv2 128/1skip, store js 74/74,
lsp_exit 3/3, prolog-lint 1. Contents: session bank a1c6edaf (10-second
law into standing laws; dataflow-rail.dl6 -- one recursive watch glob =
scope AND existence rel, 157 edges == bash referee, dangling antijoin
fired live, compile 17ms; 5 research docs: flash4 partition, refusal
inventory 245 decisions/65% weak-trail, taskmine v5+v6,
prolog-in-haskell); t4 failure-modes classes 39+40; t1
aggregate_operand_not_number refusal (2 layers, both doors); t2
support->refCount rename (25 files, vocabulary-law debt executed); t3
watchRealSource.test.ts; t5 v5 LSP exit hang FIXED (finish_lsp drops
transport instead of IoThreads::join, exit-code contract, 3 regression
tests -- ARCH row v5_lsp_exit_hang done). New ARCH rows:
aggregate_text_refusal, refcount_rename done;
cap_self_pgroup_inversion, watch_bind_hazards unbuilt. NOT merged,
intact on disk: 5 flash worktrees, flash-prolog extractor worktree
(targeted the WRONG crate -- v5 src/graph/typegraph, not
v6/sprefa-extract), every REPORT.md. AWAITING USER: prolog folder
names/numbering (plans/2026-08-01-flash4-partition-research.md +
before/after trees in chat), flash-prolog fate (redo/keep/drop),
bop-run-idle vs rail receipts (serve for watch programs, or --forever),
refusal re-eval kickoff (plans/2026-08-01-refusal-inventory.md,
lab-shaped per item), push + tag.
