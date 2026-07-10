---
name: project_engine_refactor_epic
description: the engine.rs breakdown epic — dogfood dl to measure coupling before/after a trait extraction; RelKind is Phase 1; baseline captured
metadata: 
  node_type: memory
  type: project
  originSessionId: bcd97199-357a-4558-99c0-8c9d162efb3f
---

Epic (kicked off 2026-06-29, after the ghcacher chapter closed): break down
`v5/src/engine.rs` (8765 lines, 281 fns) via TYPE DESIGN, dogfooding dl to
measure coupling before/after. Goal in Chris's words: "more/less highly coupled
yet readable with type design," plus "fuzzy matching on corresponding type vibes."

**Carried ruling (do NOT relitigate, from `docs/refactor-exploration.md` + prior
session):** the file is already laid out by who-calls-what (call-distance: current
18 vs name-sort 60 vs random 80 — reordering by name DESTROYS locality). So NO
file-split, NO gather-by-name. The surviving signal points at TRAITS, not file
cuts. node2vec is borderline here (35% naming coverage; earns its keep only ≤25%)
and merely re-derives the same trait. See [[project_refactor_exploration_dl]],
[[project_node2vec_graph_embed]].

**Trait map CONFIRMED on the live 8765-line engine.rs (both signals agree):**
- exact signature buckets (`fuzzy-traits.dl` over `type_sig`): top bucket = 25-member
  `*_rel_decls() -> Vec<RelDecl>` family = DeclProvider/RelKind (was 23, grew).
- node2vec role clusters (`node2vec-callgraph.dl`): `Db.insert_rows`/refresh
  supercluster = same RelKind role (groups by who-calls-the-same-leaf = trait).
- fuzzy "type vibes": 28-member `auto_indexes` group, 41 methods touched.

**Before-baseline — `examples/coupling-metrics.dl` (committed 8f837be, branch
feat/ghcacher-archive).** dl measures engine.rs's own dup via the `ast` backend
(structural, NOT regex per Chris's standing rule). AST call-site query that works:
`(call_expression function: (field_expression field: (field_identifier) @callee))`.
Numbers: relkind 24 consts / 24 decls / 23 used / 22 refresh = 93 members;
dispatch fan-out 40 `self.refresh_*` + 49 `*_rels_used(` = 89 sites. (AST 49 vs
grep 43 — grep needed the literal `(prog)` arg; AST is honest.) gen() writes the
versioned snapshot to `_auto-doc/coupling-metrics.md` (the dir is gitignored, the
.dl is the source of truth); re-run + diff after each phase.

**Plan:** Phase 1 `trait RelKind { fn rels()->&[&str]; fn decls()->Vec<RelDecl>;
fn refresh(&self,&mut Engine)->Result<bool> }` + 24 impls + 1 registry loop
(predicted: dispatch 89 -> ~1). Phase 2 BodyOp (scc/node2vec/closure share
fn(&mut self,&Rule)->Result) + DeclProvider. Phase 3 field-coupling sub-structs
(RepoRegistry/ClosureCache/GenJournal/RevResolver). Each gated by the dl coupling
delta.

**PROTOTYPE LANDED (2026-06-29, branch feat/ghcacher-port, UNCOMMITTED):** new
file `v5/src/relkind.rs` (310 lines) holds `pub trait RelKind` (rels/decls/
reserved_msg/refresh + default `used`) + `rel_kinds()` registry + `rel_kind_decls()`,
and the 3 git-derived families MOVED OUT WHOLE (bodies, not wrappers):
ChangedKind/ChangedLineKind/CreatedKind. engine.rs 8765 -> 8495 (-270; diff
+23/-292). Made `Engine.root` + `Engine::refresh_rel` + free `rels_used`
pub(crate) so the impls reach engine internals; `tbl` was already pub. The 6
hand-written `self.refresh_*` tick sites + 6 `*_rels_used(` gates collapsed into
2 registry loops (full tick + tick_paths); declare_builtins / all_builtin_decls /
reserved-name guard each became ONE loop over `rel_kinds()`. **Measured delta
(coupling-metrics.dl, --no-daemon, exact prediction):** relkind consts 24->21,
decls 24->21, used 23->20, refresh 22->19; dispatch_refresh 40->34, dispatch_used
49->43. Per-family linear: −4 free members + −2 dispatch sites each. Suite green
(it 368/0/4, lib 167/0/1). Reserved-msg phrase per impl preserves the old bail
text. NOT committed (this is the dl-ASSISTED arm; a separate Opus session does
the same refactor WITHOUT dl tools as the control). Fan-out to remaining 21
families is mechanical from here.

**LANDED ON MAIN (2026-06-30):**
- 3 git families (ChangedKind/ChangedLineKind/CreatedKind) committed in the
  ghcacher arc → main (259e68c "relkind: extract trait RelKind").
- **Stage 1** (ef8e0d9, main, PUSHED): RelKind absorbs 4 more zero-new-surface
  bool families — agent, type_shape, type_lgg, rel_catalog/fn_catalog. Bodies
  moved to relkind.rs; removed their consts/decls/used/refresh + ~9 dispatch
  lines. engine.rs 8495→8212. Auto-doc rail verified YELLING (dropping
  agent_edit's builtin_rel_docs line panics `undocumented_builtins`).
- **PROPOSAL doc** committed in ef8e0d9: `plans/2026-06-30-engine-breakdown-proposal.md`
  — one RelKind + optional `dirty()`/`refresh_delta()`/`refresh(cx,prog)` absorbs
  ALL 21 families; a `RelCtx` borrow-struct seam lets heavy bodies (propose/scip/
  embed) leave; target layout src/rels/{git,analysis,catalog,propose,scip,embed,
  graph,clock}.rs + tick.rs/effect.rs/source.rs/derived.rs/schema.rs; engine.rs
  ~1000-1500 target. KEY: rel families are only ~¼ of the file; the ~2000-line
  effect runtime + the tick driver are the real lift.
- **Stage 5** (578f5fc, branch worktree-engine-effect-extract off ef8e0d9, PUSHED,
  merge-tree clean into origin/main, NOT yet merged): extracted the @async/@stream/
  sh effect runtime into new `src/effect.rs` (737 lines) — EffectExec trait +
  ShellEffectExec + program registries + rebuild_async/drain_effects/drain_streams
  (kept `impl Engine`, pure relocation). engine.rs 8212→7499. Done in an isolated
  worktree because a CONCURRENT session had uncommitted op_catalog work in the
  primary checkout (op_docs()+op_catalog rel, additive, disjoint regions).
- **Stage 2** (5932b40, same branch, stacked on Stage 5, PUSHED, merge-tree clean
  into origin/main as of 2026-06-30): scip/propose_extract/propose_clone/embed
  migrated behind RelKind (ScipKind/ProposeExtractKind/ProposeCloneKind/EmbedKind;
  bodies + refresh_similar_rel helper moved to relkind.rs). RelKind gained ONE
  defaulted method `dirty(&changed)->bool` (default true); ScipKind overrides to
  gate the index reload on `index.scip ∈ seen`, replacing the scip_changed/
  wants_scip_rels plumbing in tick_paths. **DECISION: RelCtx deferred** — kept the
  existing `refresh(&self, eng:&Engine)` shape (consistent w/ Stages 0/1), widened
  5 helpers to pub(crate) (repo_roots/node_file_set/read_content/knn_rows/
  scip_descriptor_name) instead of the borrow-struct seam. RelCtx is an
  encapsulation tightening, not needed for the line move; do it later. engine.rs
  7499→7120 (−379). Gauge (same binary/worktree before→after): relkind_consts
  17→13, decls 17→13, used 16→12, refresh 15→10, dispatch_refresh 26→17,
  dispatch_used 31→23. Suite green: lib 167/0/1, it 368/0/4.
- Next per proposal: Stage 3 (split rels/ into src/rels/*), Stage 4 (bucket E:
  module/type/call/dataflow/doc/node/spine — the big extractor bodies, −1500 est,
  needs ordering+delta trait methods), Stage 6 (tick/source/derived/schema). The
  deferred RelCtx seam folds naturally into Stage 3/4.

**STATE 2026-07-02 (all prior stages ON MAIN):** relkind.rs became src/rels/
(mod.rs w/ `trait RelKind` at :82 + git/analysis/propose/scip/perf/embed/
catalog/querylog.rs, 16 impls, 1621 lines); engine.rs became src/engine/
(mod.rs 5957 + extract.rs 1246 + tick.rs 803). Measured now: relkind 14 decls/
12 used/10 refresh/14 consts, dispatch 17 refresh + 23 used — the residue is
exactly bucket E (module/type/doc/spine hand-dispatch at tick.rs:241-289 +
651-694). **Plan v2 = `plans/2026-07-02-engine-trait-refactor-v2.md`** (on the
vscode-flow-panel worktree): R1 = Stage 4 as a SEPARATE `ExtractFamily` trait
(&mut Engine refresh + digest_key + refresh_paths — don't force into RelKind),
R2 = Stage 6 seam-split of mod.rs into engine/{query,derive,source,gen,meta}.rs
w/ a 1500-line file budget, R3 = re-measure. coupling-metrics.dl extended w/
file_lines + file-size-budget diag rail (cmd/awk violation-only probe; exactly
1 file over budget today = mod.rs). Sequencing: R2 after the body-join desugar
(plans/2026-07-02-source-rule-body-join-desugar.md) — both touch the eval region.
