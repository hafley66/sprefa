# v5 Engine Refactor Suggestions

Analysis of the largest non-vendored source files (read directly, not via the dl engine).
Sizes: `engine.rs` 6152, `typegraph.rs` 2786, `propose.rs` 1607, `daemon.rs` 1226,
`modgraph.rs` 1183, `datapath.rs` 1158, `parse.rs` 893, `lib.rs` 725, `typecheck.rs` 590,
`lsp.rs` 580, `rspath.rs` 443, `scip_import.rs` 413, `ast.rs` 380, `frontend.rs` 371,
`db.rs` 348, `desc.rs` 333, `spine.rs` 332, `lower.rs` 324, `lex.rs` 227.

Findings are ordered by impact. Each: location, problem, why it matters, suggested change.

---

## Tier 0 — structural (highest impact)

### 0.1 `engine.rs` is a god-object (6152 LOC, 1 struct, ~10 responsibilities)
`Engine` conflates source ingestion, ~10 built-in indexers (module/type/call/dataflow/doc/spine),
CST walk, closure condensation, gen/splice/cursor writes, repo config, and daemon-state persistence.
12+ field types, touches 30+ tables, 50+ methods.

- **Why:** testing one concern needs a full `Engine`; a new indexer means editing `tick()` and
  `tick_paths()` in lockstep; parallel work collides on `Engine` borrows.
- **Refactor:** split into modules behind a thin facade —
  `sources.rs` (`SourceReconciler`: reconcile/retract/parse_file),
  `builtin_indexers/` (`ModuleIndexer`/`TypeIndexer`/`CallIndexer`/`DataflowIndexer`, each `fn refresh(&self, db, rels) -> Result<bool>`),
  `closure_eval.rs` (`ClosureEvaluator`), `gen_engine.rs` (`GenWriter`). `Engine` becomes a coordinator that
  owns these and runs them in `tick()`.

### 0.2 `tick()` (1645–1836, 191 LOC) and `tick_paths()` (1842–2116, 274 LOC) are copy-paste twins
`tick_paths` duplicates the 10+ `if module_rels_used(prog) { … } if type_rels_used(prog) { … }`
conditional refreshers with added path filtering. The incremental CST walker exists *only* in
`tick_paths` (≈2021).

- **Why:** every new indexer must be added in both; the two diverge silently (digest pruning at ≈1991 is
  path-only), so a test green on `tick()` can fail on `tick_paths()`.
- **Refactor:** `trait Indexer { fn needs_refresh(&self, prog, changed) -> bool; fn refresh(&mut self, mode: TickMode, ...) -> Result<bool>; }`
  plus a `TickPlan { mode: Full | Paths(&[PathBuf]), indexers_to_run }`. One loop over indexers in both
  entry points; full vs delta is a per-indexer config, not a forked function.

### 0.3 Four near-identical corpus-refresh pipelines
`refresh_type_rels` (3336–3451), `refresh_call_rels` (3715–3841), `refresh_doc_rels` (3932–4043),
`refresh_dataflow_rels` (3864–3924) all follow: query `_file` → `par_iter` extract → build corpus
indices (by_name/sym_at) → resolve → emit deduped rows → `refresh_rel` per sub-rel → rebuild legacy mirror.

- **Why:** a fix to corpus-index logic = 4 edits; a new fact type = 4 row-building loops.
- **Refactor:** `trait CorpusIndexer { type Facts; fn extract_all(files) -> Vec<Facts>; fn build_indices(&[Facts]); fn emit_rows(facts, idx); fn refresh_rels(db, rows); }`.
  One driver loop over indexers.

### 0.4 `parse_file()` (5640–5948, 308 LOC) — 13-arm match with inline spine interning
Each `BodyItem` variant repeats extract → bind → intern-captures → push. Spine interning runs per-row
inside the loop (violates collect-then-flush; project CLAUDE.md flags this).

- **Why:** a new extractor = copy an ~80-LOC arm and remember to intern; per-row `push_span` on files with
  100k matches.
- **Refactor:** `trait Extractor { fn extract(&self, content, binds) -> Vec<Bind>; fn interns(&self) -> Vec<(WhereBytes, String)>; }`,
  one impl per variant. `parse_file` loops once, then batch-flushes `interns()` outside the loop.

---

## Tier 1 — duplication across languages / formats

### 1.1 `typegraph.rs` parses each file 3× per tick
`extract`, `extract_calls`, `extract_dataflow` each re-parse independently (comments at 251/273/305/868
acknowledge it).
- **Refactor:** `parse_once(lang, content) -> Ir` (enum over `syn::File | tree_sitter::Tree | oxc Program`),
  thread the IR through all three phases.

### 1.2 Three parallel dataflow walkers (`flow_kt` 325–515, `ts_flow_expr` 537–877, `flow_expr` 2016–2340)
Identical lift semantics (child→parent + slot binding), reimplemented per language; `LoopFact`
construction duplicated (497/726/2263).
- **Refactor:** `trait FlowNode { kind/children/text }` + one generic `lift_flow<N: FlowNode>(...)`.
  Adapters `impl FlowNode for {tree_sitter::Node, syn::Expr, oxc node}`. Extract `mint_loop_fact(...)`.

### 1.3 Stringly-typed edge kinds in `typegraph.rs`
`BTreeSet<(String, String, &'static str)>` with `"field"`/`"variant"`/`"impl"`/`"generic"`/`"param"`/`"returns"`/`"uses"`
hardcoded at each call site; type-ref collection duplicated (Rust 920–1156 / Kotlin 1181–1314 / TS 1350–1668).
- **Refactor:** `enum EdgeKind { … }` with `.as_str()`; `collect_type_refs<N: TypeNode>(...)` shared across languages.

### 1.4 `propose.rs` — nine copy-paste `*_proposals` functions (54/225/377/482/557/632/755/850)
Only the feature extractor differs (ast_shape/subtree_hash/cfg/callgraph/ddg/ngram/leaf/symbol). Parser
setup, statement ranges, matching, free_vars, ranking, output all duplicated. Gain formula
`n*occ.saturating_sub(1)` recomputed in all nine.
- **Refactor:** `generic_clone_proposals<T: FeatureExtractor>(content, extractor)`; one call site.
  `impl Proposal { fn compute_gain(lo, hi, occ) }`. Also unify the three `*_shape_tokens` walks into
  `tree_leaves<F>(tree, content, normalize)`.

### 1.5 `modgraph.rs` — three resolver `edges()` are the same shape (Rust 245–296 / TS 704–740 / Kotlin 874–920)
strip-noise → iterate regex captures → extract specifier/line/span → resolve → build `ModuleRef`.
- **Refactor:** `trait Extractor { fn extract_specifiers(content) -> Vec<(String,u32,usize,usize)>; }`
  + generic `resolve_edges(file, content, &dyn Extractor, &dyn Resolver)`.

### 1.6 `datapath.rs` — `entries` (98–150) vs `entries_kd` (832–891) ~150 LOC duplicated
Same Fmt-branching; only difference is `entries_kd` tracks key span + joins segments.
- **Refactor:** one `entries_with_span(...) -> Vec<(Vec<String>, (usize,usize), Node)>`; `entries` projects away the span.
- Also: three unescape fns (json 212 / yaml 245 / toml 308) overlap → `unescape(s, EscapeStyle)`.

### 1.7 `daemon.rs` — duplicated broadcast + reload + tick pairs
`broadcast_diag_changed` (250–276) / `broadcast_rev_advanced` (361–381) copy the lock-iterate-trylock-reap
loop; `reload_program` (177–197) / `reload_discovery` (203–248) copy the type-error check; `tick_full`
(146–157) / `tick_paths` (160–172) each do 3 lock acquire/release cycles.
- **Refactor:** `broadcast_notification(&self, note: Value)`; `validate_and_apply_program(new_prog, diags)`;
  `tick_impl(&self, paths: Option<&[PathBuf]>)` holding both locks once.

---

## Tier 2 — long functions / state machines

| File | Function | Lines | Issue |
|------|----------|-------|-------|
| daemon.rs | `handle_request` | 810–1005 (196) | 11-arm RPC match; `load`/`eval` both build ephemeral engines without sharing |
| daemon.rs | `watcher_loop` | 549–726 (178) | FSEvent batch + classify + program/config/git dispatch + discovery merge in one |
| lex.rs | `lex` | 67–227 (160) | monolithic char-dispatch; 42-LOC inline string parse; `/` regex-vs-divide lookahead inspects output vec |
| typecheck.rs | `check_rule_types` | 272–420 (148) | 132-LOC nested closure, 8+ Term arms, narrowing logic repeated 3× |
| parse.rs | `gen_rule` | 537–605 (68) | two forks (byte-splice vs file/line) build `GenTarget` separately |
| lower.rs | `body_sql` | 66–168 (102) | Pos/Neg/Cmp passes repeat term-matching 3× |

- **daemon `handle_request`:** `struct RequestHandler<'a>(&Daemon)` with one method per RPC; shared
  `run_ephemeral_engine()` for `load`+`eval`.
- **daemon `watcher_loop`:** extract `batch_watcher_events`, `classify_event(...) -> EventKind`, and a
  handler per event kind.
- **lex `lex`:** factor `lex_string`/`lex_regex`/`lex_operator`; track a context enum for `/` instead of
  reading the emitted-token vec.
- **typecheck `check_rule_types`:** per-Term check fns; extract
  `bind_var_narrowing(seen, var, cty, …)` (currently duplicated at 323/351/399).
- **lower `body_sql`:** one `handle_term(term, cell, &mut canon, &mut wheres)` used by all three passes.

---

## Tier 3 — seams and small mechanical wins

### 3.1 `db.rs` seam is leaky
- `insert_rows` wraps `BEGIN`/`COMMIT`/`ROLLBACK` via raw `conn.execute_batch` (239/253/258), bypassing the
  `bump()` counter — transaction overhead is invisible to the N+1 detector.
  **Refactor:** `transact_batch(f)` that bumps a synthetic `"TRANSACT"` key.
- `pub fn conn()` (161) is an unguarded bypass of the whole seam; only `grep .conn()` audits it. ~50 sites in
  `engine.rs` (1282, 1303, 1323, 1350, 1404, 1416–1420, …). Most are diagnostics (per-rel `COUNT` loops, e.g.
  1826). **Refactor:** add `Db::count_rels(&[&str])` for the count loops; mark remaining diagnostic sites with
  a `DiagnosticConn` marker or `// AUDITED` so hot-path additions stand out.

### 3.2 Spine hashing repeated (`spine.rs` 66/86/114)
`RefId::of_coord`, `WhereBytesId::of`, `of_located` each hand-sequence `h.update()` then `first_u64`.
**Refactor:** `hash_fields(&[&[u8]]) -> u64`.

### 3.3 `rspath.rs` module-path walking duplicated (95–115 vs 120–147)
`resolve_to_absolute` and `reconvert_prefix` both pop `super::` segments + rejoin.
**Refactor:** `pop_module_path(path, count)` + `join_path_segments(base, rest)`.

### 3.4 `lsp.rs` position math is O(n) per call and triple-implemented
`position_to_byte` (364–378), `byte_to_line_col` (414–424), `span_to_range` (382–386, runs the scan twice).
**Refactor:** build a per-file `LineIndex` once per request, cache in a map; reuse for all conversions.
Also: `percent_encode`/`percent_decode` (468–493) → `urlencoding` crate. Handler dispatch
(`handle_definition`/`handle_hover`/`handle_references`, 257–341) → `trait RequestHandler` + registry.

### 3.5 `scip_import.rs::rows` (51–156) walks each document twice
Phase 1 collects defs (52–83), phase 2 processes refs (85–137).
**Refactor:** single pass; extract `process_local_def(...)`.

### 3.6 `desc.rs` char-escape iteration + segment rendering duplicated
`split_unescaped_slash` (153–169) vs `unescape` (206–217); `render_pattern` vs `render_concrete` (271–290).
**Refactor:** `unescape_chars(s) -> impl Iterator<Item=char>`; `render_segments<F>(segs, renderer)`.

### 3.7 Frontend term-walk written by hand (fragile completeness)
`frontend.rs::rewrite_terms` (206–259) manually lists every `BodyItem` field; a new field silently skips
rewriting. Same hazard in `typecheck.rs::normalize_body_item` (194–235).
**Refactor:** `trait WalkTerms { fn walk_terms_mut(&mut self, f: &mut impl FnMut(&mut Term)); }` on `BodyItem`,
used by both. `inline_template_calls` (157–201) manual-index `splice` → build a new `Vec` via flat_map.

### 3.8 Source-op name lists out of sync
`parse.rs::body_item` (291–320) hardcodes source-op names; `ast.rs::Rule::is_source` (219–224) re-lists them.
A name added to one and missed in the other parses but misclassifies.
**Refactor:** `enum SourceOp` used by both; or a single `const SOURCE_OPS: &[&str]`.

### 3.9 Misc
- `parse.rs`: ~10 hand-rolled comma-loops → `parse_delimited<T>(delim, term, parse_one)`.
- `parse.rs`: `head_atom` parallel `aggs` vec (228) leaks into lowering (`lower.rs` 191–219) →
  `Vec<AggTerm>` with `enum AggTerm { Plain(Term), Agg(AggFn, Term) }`.
- `lower.rs::term_sql` (32–61): function whitelist (`split`/`replace`/`int`) hardcoded in body + error message →
  `const FUNCTIONS: &[(&str, usize)]`.
- `modgraph.rs::strip_noise` (126–207, 82 LOC): per-quote-type inline state machine →
  `skip_line_comment`/`skip_block_comment`/`skip_raw_string` helpers.
