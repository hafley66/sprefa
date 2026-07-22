# Session digest — the reactive/incremental/graph-algorithm lineage

Every important discussion about **salsa · DRed · differential-dataflow · timely ·
dataflow · async/sync · Z-sets/feldera/DBSP · counting-retraction · SCC/reachability
/closure · fixpoint/semi-naive · perf & Big-O**, distilled from the 226 chat_log
session files (78 relevant). Each bullet ends with its source `chat_log/<file>`.
Read top-to-bottom = the timeline of how we got to "one counting cascade in SQLite".

Mined by 8 haiku passes over chat_log/*.md. Rebuild: re-run the batch dispatch
(see git history for this file's commit). Companion: `../DECISIONS.md` (the settled
call), `../ARCHITECTURE.md` (the one-cascade model).

---


### 20260501.0 — dd-adoption-effects-control-flow-types-perf
- Three-thread-pool design: timely (single-thread sync per worker, dataflow) vs rayon (CPU-bound parsing) vs tokio (I/O dispatch); coordinate via push+oneshot channels. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- DD set semantics + retraction auto-clears diagnostics; set-without-retraction gap is what current sprefa-v3 relation_store has, DD closes it. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- Frontier `step_while(probe<T+1)` is principled boundary seal, replaces homegrown RAII-writer-share + seal_waiters. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- Retractable effects via three regimes: idempotent terminal (already works), debounced commit (effect_pending Collection), paused yield (DD collection mirror + antijoin on resolved+upstream+timed_out). (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- Events as gen-only via auto-retract: default impl (+1 at T, scheduled -1 at T+1); iterate scope reserved for cascade blocks with parse-time acyclicity check. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- DD memory at 500-repo scale: Strategy 4 tiered (DD holds metadata+IDs, SQLite holds content, trigram for grep) avoids raw strings in trace; realistic RSS 5–8 GB on 50 GB source. Strategy 3 (custom Trace) deferred to measurement. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)
- Compaction is only memory tool DD provides; K_GENS=5, M=10 reasonable starting point. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260503.2 — v4-dd-store-interner-strings-indexer-substrate
- DD time-domain bug: DdStore calling `input.advance_to(gen+1)` on commit but `advance_to(gen)` on insert caused dropped rows after first commit; fixed by maintaining monotonic `dd_time: Gen` per-store. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)
- DD vs MemStore cost crossover: DD wins reactive shape with frequent small commits; mem wins when `rules × commits × rows < ~2k`. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)
- Retraction (Z-sets) proof point: per-commit deltas propagate through rule outputs via +1/-1 diffs; MemStore rederive ~220ns/row-touch, DD step ~µs at small delta scaling to ~ms with delta size. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)
- u32-keyed DdRow compression closes 8-byte-per-term bloat; Interner fwd+rev mapping (HashMap<Arc<str>, u32> + Vec<Arc<str>>) enables stateless id sharing across processes. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)

### 20260503.3 — v4-sqlite-poc-fact-rule-two-tick-content-hash
- Two-tick reactive model (render phase vs commit phase) mirrors React commit semantics; SqliteStore delays writes until explicit commit, separates incremental compute from persistence boundary. (`20260503.3.v4-sqlite-poc-fact-rule-two-tick-content-hash.md`)
- Three-layer caching design: sprf-source check (unchanged pipeline → no-op), per-rule input set hash (skip unchanged rules), per-op cache by `(sprf_path, input_lineage_hash)` for partial reuse within invalidated rule. (`20260503.3.v4-sqlite-poc-fact-rule-two-tick-content-hash.md`)
- OpCache hit on `(op_ident, lineage)` replaces fiber.key re-render bailout; content-hash stamping in AstNm anchors downstream caches (same content → same hash → same lineage → cache hit). (`20260503.3.v4-sqlite-poc-fact-rule-two-tick-content-hash.md`)
- Switchmap cancellation only safe with granular invalidation; async renderer cost asymmetry (20+ seconds cold vs ~16ms React) recoverable via layer-1/2 caching + content hash on every source op. (`20260503.3.v4-sqlite-poc-fact-rule-two-tick-content-hash.md`)

### 20260504.10 — fact-wide-tables-and-fine-grain-invalidation
- Cascade-delete must work after root popped; Sqlite CTE seed = `id=root OR parent_id=root`, Mem uses parent_id matching from frontier check. (`20260504.10.fact-wide-tables-and-fine-grain-invalidation.md`)
- B2 positional identity (row.path-keyed) survives cache turnover vs B1 MemoKey dies on eviction; position-stable identity is React-key shape. (`20260504.10.fact-wide-tables-and-fine-grain-invalidation.md`)
- Memoize MUST preserve inner.render_batch; naive per-row dispatch breaks AstNm tier-2 work; fix: pre-classify hits/misses, dedupe identical inputs, ONE render_batch call for unique misses. (`20260504.10.fact-wide-tables-and-fine-grain-invalidation.md`)
- Reconcile opt-in within opt-in (Memoize itself opt-in, PriorChildIndex beyond that); scan mode assumes downstream alive (wrong for trials 2+) but LSP/watch with persistent parks correct. (`20260504.10.fact-wide-tables-and-fine-grain-invalidation.md`)

### 20260505.0 — linux-bench-tail-sync-cap-fix
- Incremental compute bottleneck: inter-batch tail sync (N-1 workers idle waiting on slowest file between batches); one mega-batch (batch_cap ≥ 65536) eliminates tail-sync points, rayon work-stealing handles all files at once. (`20260505.0.linux-bench-tail-sync-cap-fix.md`)
- Batch serialization vs async overhead distinction: 70% of 81% latency gap is pipeline alternation deadlock + sync I/O blocking runtime; only ~11% is tokio overhead vs raw rayon. (`20260503.0.v4-poc-engine-pipeline-buffer-perf.md`)
- Buffered channels between ops (tokio::sync::mpsc::channel, ReceiverStream) enable producer N batches ahead of consumer; solves Fs enumeration + AstNm parsing alternation. (`20260503.0.v4-poc-engine-pipeline-buffer-perf.md`)

### 20260505.3 — evolution-log-v1-v2-v3-v4
- DD proofs prototyped, parked: "DD's only remaining win is incrementally updating in-memory aggregate; for sprefa's debounced-commit/indexed-query/settled-state workload, recompute on commit beats incremental." (`20260505.3.evolution-log-v1-v2-v3-v4.md`)
- No sync fs/git calls except initial seed; SQL behind trait; ops emit cursors, sinks buffer+bulk-write; no per-row updates. (`20260505.3.evolution-log-v1-v2-v3-v4.md`)

---



### 20260507.2 — v4 rule-engine respec and memory audit
- Engine model: body cache-fill thunk keyed by input-args, mode dispatch on query `?` positions, watcher registry handles live update, retract chain on source-change. **This IS differential dataflow with Datalog surface.** (`20260507.2.v4-rule-engine-respec-and-memory-audit.md`)

### 20260515.4 — Runtime-graph next steps reconciled
- **JoinCore::join_core performs incremental hash-join with O(delta) work per update;** Iterate::iterate performs semi-naive recursion: each iteration processes only delta from previous step, not full transitive frontier. (`20260515.4.runtime-graph-next-steps-reconciled.md`)

### 20260515.4 — Runtime-graph next steps reconciled (DD adoption)
- DD-backed `materialized_join` operator as opaque sink behind feature flag, **DD holds metadata + Arc<str> IDs + relational graph; SQLite holds canonical string content; trigram index for bulk grep.** Realistic live RSS 5–8 GB on 50 GB source at 500-repo scale with Strategy 4 (tiered memory). (`20260515.4.runtime-graph-next-steps-reconciled.md`)

### 20260501.0 — dd-adoption effects control-flow types perf
- Three thread pools (rayon CPU-bound parse, timely single-thread dataflow, tokio effects I/O), scope + sinks + probe coordinated via push + oneshot channels; **set semantics + retraction makes auto-clearing diagnostics fall out for free.** (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260501.0 — dd-adoption effects control-flow types perf (Retraction)
- Three retractable effect regimes: (1) idempotent terminal (set semantics), (2) debounced commit (effect_pending Collection, commit at frontier T+K, retract within window = free cancel), (3) paused yield (mirror SubjectRegistry as DD collections, antijoin against resolved + upstream + timed_out). (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260501.0 — dd-adoption effects control-flow types perf (Pure operators)
- Purity contract: deterministic, total, side-effect free, time-pure, build-time captures only; three seams of impurity (rayon input, inspect_batch output, tokio effect dispatch); five durability capabilities follow: input log source of truth, operator graph swap, time-travel queries, splittable replay, effect idempotence. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260501.0 — dd-adoption effects control-flow types perf (Memory and fixpoint)
- **DD's iterate is the only cyclic operator; default to NOT using it, reserve for cascade blocks with parse-time acyclicity check.** **Compaction is the only memory tool DD gives you;** set policy explicitly, K_GENS=5, M=10 reasonable starting point. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260501.0 — dd-adoption effects control-flow types perf (Events and frontier)
- Events as gen-only via auto-retract: **default impl +1 at T, scheduled -1 at T+1 by runner; within-time fan-out gives "all consumers see all emits at T regardless of arrival order" for free.** Frontier `step_while(probe<T+1)` replaces homegrown RAII-writer-share + seal_waiters mechanism. (`20260501.0.dd-adoption-effects-control-flow-types-perf.md`)

### 20260503.2 — v4 DdStore + Interner + perf model for strings-indexer substrate
- DD reactive store with u32-keyed rows (`Vec<(u32, u32)>`, 8 bytes per term); **crossover where DD wins: reactive shape with frequent small commits; rule work 12× less at medium scale (commits-per-row high), tied at large scale (delta IS most input).** Cost model: MEM rederive ~220 ns/row-touch, DD step ~µs at small delta to ~ms at large delta. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)

### 20260503.2 — v4 DdStore (Big-O and memory)
- DD RSS post-u32: ~500 B/row at large scale matching MEM; **crossover (mem wins): rules × commits × rows < ~2k; crossover (dd wins): reactive shape with frequent small commits.** Time-domain bug: DD was silently dropping inserts when caller gen lagged DD's frontier. (`20260503.2.v4-dd-store-interner-strings-indexer-substrate.md`)

### 20260505.3 — Evolution log v1-v4
- v3 historical stance: "**Differential-dataflow proofs prototyped — parked with conclusion DD's only remaining win is incrementally updating an in-memory aggregate; for sprf's debounced-commit / indexed-query / settled-state workload, recompute on commit beats incremental.**" (`20260505.3.evolution-log-v1-v2-v3-v4.md`)

### 20260514.0 — v4 lazy semantic graph frontier plan
- Graph algorithms over SQLite-backed rows: reachability (BFS, blast-radius query), **strongly connected components (scc)**, bounded path explanation, neighbors (one-hop), graph rendering; lazy stack materializes typed graph facts only for reachable frontier rather than eagerly parsing all repos. (`20260514.0.v4-lazy-semantic-graph-frontier-plan.md`)

### 20260515.3 — graph-audit pipey-rx-reconcile
- **DD's RAM cost is per-key trace history; with `Trace::set_logical_compaction` bounded by retained logical times — tractable for one operator (join/materialize), not whole graph.** Reactive philosophy: three influences merged intentionally: RxJS (operators that can complete) + React (pure renders, Suspense) + redux-saga (dispatch/render/render_batch). (`20260515.3.graph-audit-pipey-rx-reconcile.md`)

### 20260516.4 — sprefa-v4-audit-runtime-theory
- **run_to_quiescence = Kleene iteration of T_P to lfp; IterationCap = unsound truncation.** Negation breaks monotonicity; stratification (level map, no negative-edge cycle) restores unique perfect model, closing recursion risk. **ΔT_P/semi-naive = DD first-order.** Support edge = JTMS justification/provenance-semiring; DD constant-RSS lever is compaction/frontier advancement (quotient history below frontier); Adapton/Salsa dual = less RAM, more recompute. (`20260516.4.sprefa-v4-audit-runtime-theory.md`)

---



### 20260516.4 — v4 audit runtime theory
- DD's Support edge (JTMS justification / provenance-semiring) enables differential correctness via retraction; Support-recompute model is Adapton/Salsa dual (less RAM, more recompute). `(20260516.4.sprefa-v4-audit-runtime-theory.md)`
- Stratification (level-map rule graph, reject negative-edge cycles at compile-time) restores unique perfect model for negation; IterationCap is unsound truncation. `(20260516.4.sprefa-v4-audit-runtime-theory.md)`
- Runs-to-quiescence (Kleene iteration of T_P to lfp) is the fixpoint operator; semi-naive (ΔT_P) modeled as re-fire/expand+queue. `(20260516.4.sprefa-v4-audit-runtime-theory.md)`

### 20260517 — query memoization ramifications
- DD arrangement = materialized, index-organized, retraction-aware view; mount table is sprefa analogue with support edges playing reference counts and retract_supported_rows playing arrangement compaction. `(20260517.query-memoization-ramifications.md)`
- Two consumers sharing a mount (DD-faithful) requires multiplicity-aware fan-out; sprefa's mount keyed by SQL text accidentally replicated DD's arrangement dedup without multiplicity bookkeeping. `(20260517.query-memoization-ramifications.md)`

### 20260518.3 — memo content stale into main
- DD constant-RSS lever is compaction/frontier advancement (quotient history group below frontier); user's Support-recompute model is Adapton/Salsa dual. `(20260518.3.sprf-memo-content-stale-into-main.md)`
- O(1) git-token staleness via content-hash + mtime fast-path; no DD on hot path but DD crates remain in Cargo.toml until deletion (dependency-true deletion needed). `(20260518.3.sprf-memo-content-stale-into-main.md)`

### 20260518.6 — recursion reactor graph examples
- Negation needs SQL WHERE NOT EXISTS because a bare rule read is always semi-join (flow-per-match); there is no bare spelling for "drop if any match" (antijoin). `(20260518.6.sprf-recursion-reactor-graph-examples.md)`
- Incremental skip soundness: warm_changed_paths is corpus-wide not per-rule; Some([]) is NOT proof a given recursive rule's sources are unchanged. Default recompute, env-gated skip. `(20260518.6.sprf-recursion-reactor-graph-examples.md)`

### 20260520.0 — n-plus-1 rule factwrite budget test
- Deterministic budget tests catch N+1 SQLite inserts; 1000 rows → 1008 insert_batch calls (avg 4.5 rows/call) in unit test that reproduces 10-minute bench regression. `(20260520.0.n-plus-1-rule-factwrite-budget-test.md)`

### 20260520.1 — v4 n-plus-1 fixes budget tests reactor plans
- SupportLedger::add_batch replaces per-row ledger.add loop; resolved_root_cache DashMap memo collapses 63k per-row git rev-parse calls to 1 fork via stat-walk for .git. `(20260520.1.20260520.1-v4-n-plus-1-fixes-budget-tests-reactor-plans.md)`
- Plural-API + collect-then-flush (not async DataLoader) is the discipline; FsComponent already chunks via self.batch and flush(); queue peak not FS-side memory is the real concern at scale. `(20260520.1.20260520.1-v4-n-plus-1-fixes-budget-tests-reactor-plans.md)`

### 20260521.0 — v5 dl reactive datalog engine
- **COUNTING breaks on cycles (self-support lie) but is SOUND on a DAG → condense each cycle (SCC) first, then counting works.** Reaches recursion = 197s naive, SCC-condensed = 1.4ms (~130,000x). `(20260521.0.v5-dl-reactive-datalog-engine.md)`
- DD constant-RSS lever (compaction/frontier) fails on single board; Glean's ownership-set model = _prov extended through derivation DAG (surgical-incremental upgrade). Nobody embeddable nails scale+RSS+durability. `(20260521.0.v5-dl-reactive-datalog-engine.md)`
- Differential dataflow keeps RAM-resident arrangements; Motik B/F exact but resident; RSS-frugal path = SCC-condensed on SQLite (IncQuery IncSCC) + don't store E+, reconstruct on demand. `(20260521.0.v5-dl-reactive-datalog-engine.md)`

### 20260521.3 — v5 scc stratify cgrammar autoindex
- **SCC Tarjan pass runs on multiple graphs (data call graph=reach, rule dependency graph=stratification, indexing=NOT a cycle problem, a cover).**  Souffle's speed is tech stack (C++, auto-indexed B-tree/Brie, interning, OpenMP), NOT better closure algorithm. `(20260521.3.v5-scc-stratify-cgrammar-autoindex.md)`
- Stratified negation makes globally non-monotone program a stack of locally-monotone fixpoints; reject negation inside a recursive cycle. Recursive-CTE VIEW cannot seed point queries; seeded Rust walk (reaches_from/reached_by) is 30us vs 1.98s. `(20260521.3.v5-scc-stratify-cgrammar-autoindex.md)`
- Auto-index: Souffle-style (precise = min-chain-cover/Dilworth); kernel 30s → 9.7s via every column a var reaches across ≥2 body atoms. Semi-naive FRONTIER (join only new facts per round) still pending. `(20260521.3.v5-scc-stratify-cgrammar-autoindex.md)`

### 20260530.0 — v5 lsp diag ban gate and relation digest plan
- Relation input-digest skip (Axis-A reconcile scaling): per-relation digest = XOR-fold of __src over rel_<R> (order-independent, set PK makes no dup cancels); if equal to stored digest, prune from changed_source_rels. `(20260530.0.v5-lsp-diag-ban-gate-and-relation-digest-plan.md)`
- v5 fixpoint has hard round cap (iters > 100_000 bail) matching v4's no-hang lesson; k8s-apply mental model correct: .dl=desired-state, tick=reconcile loop, fs/git=observed state, SQLite=etcd, LEVEL-triggered (hash diff) with edge-triggered wakeup (fs event). `(20260530.0.v5-lsp-diag-ban-gate-and-relation-digest-plan.md)`

### 20260531.3 — net-lab solid engine design
- Graph-to-tree projection: compute parent via BFS/DFS spanning tree or dominator tree, with SCC condensation (Tarjan) folding cycles to one entry (what dl does for reachability). `(20260531.3.net-lab-solid-engine-design.md)`

### 20260531.4 — v5 architecture glean plus plus and db seam
- **DD is the WRONG fit: in-RAM arrangements are the RAM problem; Materialize spilled to disk (VLDB 2023) to survive. Glean disk-backed (RocksDB), NOT DD-in-RAM; keep trace in SQLite (paged), not RAM.** `(20260531.4.v5-architecture-glean-plus-plus-and-db-seam.md)`
- Take Salsa red-green semantics + SQLite trace (not RAM); digest-skip IS red-green early-cutoff. v5 position: Glean disk-backed + Salsa red-green + Souffle bottom-up fixpoint (SQL INSERT-until-zero) − DD in-RAM arrangements. `(20260531.4.v5-architecture-glean-plus-plus-and-db-seam.md)`
- Peak-RSS invariant ≈ max(parse working set, condensation adjacency, SQLite cache) — none is corpus. Cold-build concern; RAM/speed knob = Rust condensation (fast spike) vs SQL recursive-CTE view (paged bounded). `(20260531.4.v5-architecture-glean-plus-plus-and-db-seam.md)`
- N+1 discipline (plural-API + collect-then-flush): Db is single SQL owner; insert_rows = chunked multi-row VALUES = one op; per-tick counter screams >64x same statement; conn() = metered escape hatch. `(20260531.4.v5-architecture-glean-plus-plus-and-db-seam.md)`

---

(Sessions with substantive content on specified topics: 16.4, 17, 18.3, 18.6, 20.0, 20.1, 21.0, 21.3, 30.0, 31.3, 31.4)


### 20260602.0 — glean-showpiece + portable-reactive-core + DD peak-RSS
- Differential-dataflow excluded on memory grounds: 4x trace amplification at 500-repo scale; compaction discards time-travel (the value of DD). (`20260602.0.v5-glean-showpiece-where-kill-jsonout-portable-core-research.md`)
- Salsa (memoization shape, not relational retraction) insufficient for incremental+retraction problem; no portable lib bundles graph-algos + datalog + incremental-retraction in non-DD shape. (`20260602.0.v5-glean-showpiece-where-kill-jsonout-portable-core-research.md`)
- Semi-naive evaluation already in rebuild_derived; portable core = petgraph (static math) + datafrog (batch datalog) behind RelStore seam + v5's own sync-tick retraction. (`20260602.0.v5-glean-showpiece-where-kill-jsonout-portable-core-research.md`)
- Transitive closure O(V²) fatal at 10M nodes; stack = SCC condensation + bounded-depth + hierarchical + semi-naive evaluation. (`20260602.0.v5-glean-showpiece-where-kill-jsonout-portable-core-research.md`)

### 20260619.2 — oracle-grounded-lattice
- Semi-naive evaluation already implemented in rebuild_derived; differential dataflow excluded on RAM grounds due to peak-RSS amplification. (`20260619.2.sprefa-oracle-grounded-lattice.md`)
- SCC condensation proven within compression stack; closure runs on DAG not on cycles. (`20260619.2.sprefa-oracle-grounded-lattice.md`)
- Bounded-depth reachability O(kE) for 2-3 hop queries; per-file closure + boundary merge matches real code locality (flow stays in-file 95%+). (`20260619.2.sprefa-oracle-grounded-lattice.md`)

### 20260619.3 — cursor + rails + CST-architecture
- Engine's closure-in-body restriction forces recursive derived rules over base edge; recursive rules run on semi-naive fixpoint (slower than SCC-condensed closure but fine for ~100 fns). (`20260619.3.v4-cursor-and-loop-reachability-rails.md`)

### 20260721.1 — v6-store-sqlite-cascade-ivm-lab
- Feldera-style Z-set retraction via weight cascade: delta-proportional work ∝ change; SQLite on-disk completes where differential-dataflow and DBSP abort at 1.5GB budget. (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)
- Counting Z-set retraction correct only on acyclic graphs; cycles self-sustain (phantom). DRed fixes cycles; DD correct by construction. (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)
- WITH RECURSIVE recompute 720x slower (@16k) than incremental cascade due to path-explosion; byte-identical but rejected. (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)

### 20260721.3 — v6-core-frp-edge-vs-batch-decision
- v5 ALREADY is a reactive memo engine: FamilyRouter = MobX/Solid computed + rayon batch + bounded-channel backpressure (crude: no throttle/buffer/distinct). (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)
- Pull primitive + invalidation-as-push + backpressure = Salsa pattern; batch-relational core unsuitable for FRP (max pain/min payoff); FRP belongs at event edge (bounded pain/max payoff). (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)
- v5 stratifies extraction by family for ast-grep rayon maximization; streams-first would shatter the batch (DataLoader trap). (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)

### 20260721.4 — v6-sqlite-dd-dred-cycle-safe-relstore-backport
- Delete-and-Rederive (DRed): on delete, over-delete forward cone (tentatively unreach cycles included), then rederive any cone node still reachable from surviving anchor. Correct on any graph; cost ∝ cone size. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)
- Wavefront cost is universal lower bound |Δoutput|; no engine beats it if materializing full answer. DD pays in resident RAM (bounded), SQLite on disk. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)
- Salsa pattern in SQL: rx_memo(id, digest, changed_at, verified_at) + rx_dep(reader, read); dirty() = stale frontier via dep-based rule; verify() = early-cutoff. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)
- Lazy frontier (dirty() dep-based, not self-changed) + verify-moves-changed_at = early cutoff for demand-driven laziness. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)



### 20260625.2 — cst-node-child-perf-nested-set-ancestry-experiment
- CST = forest + acyclic + nested spans => nested-set encoding => ancestry/containment is a span RANGE JOIN, not closure(child)/SCC. `(20260625.2.cst-node-child-perf-nested-set-ancestry-experiment.md)`
- General fully-dynamic SCC is hard (delete-splits ~ research-grade); for genuinely cyclic graphs (type_edge/module_edge/call_edge) full recompute is cheap because they're far smaller than 136K. `(20260625.2.cst-node-child-perf-nested-set-ancestry-experiment.md)`
- closure() ALWAYS SCC-condenses (engine.rs dedup_edges); wasted on acyclic child, hence the 0.82s residual after incremental perf fix. `(20260625.2.cst-node-child-perf-nested-set-ancestry-experiment.md)`

### 20260625.3 — cst-merged-dogfood-cutover-ancestry-experiment
- ANCESTRY (measured): interval/nested-set span predicate wins for POINT/innermost-containment (LSP query; free, no SCC) but LOSES to closure(child) for FULL anc materialization (91ms vs 484ms; unindexed node×node self-join is O(n²), hangs at 136K). `(20260625.3.cst-merged-dogfood-cutover-ancestry-experiment.md)`
- CST is forest + acyclic + nested spans; closure() ALWAYS SCC-condenses — wasted on acyclic child, hence the 0.82s cost after path-scoped incremental fix. `(20260625.3.cst-merged-dogfood-cutover-ancestry-experiment.md)`
- path-scoped incremental refresh (node_delta_paths mirrors module_rels_for_paths) dropped incremental 2.8s→0.82s; structural guard test asserts node walk is path-scoped on single-file edit. `(20260625.3.cst-merged-dogfood-cutover-ancestry-experiment.md)`

### 20260625.5 — christmas1-data-driven-scan-repo-sink-tracing-4b
- 1-tick latency for data-driven scan + repo-sink: coord/pull relation is derived AFTER source; scan/pull reads LAST tick's relation, avoiding source→derived→source fixpoint rewrite. `(20260625.5.christmas1-data-driven-scan-repo-sink-tracing-4b.md)`
- Reactive conditions via explicit recursion for data-driven scan coordinates: scope rows bound per-tick, reused in upstream filter, enabling multi-binding. `(20260625.5.christmas1-data-driven-scan-repo-sink-tracing-4b.md)`

### 20260626.0 — scc-builtin-auto-refactor-discovery
- closure runs Tarjan internally for its own condensation; scc exposes that partition => zero second Tarjan, shared closure_cache[edge].cond. `(20260626.0.scc-builtin-auto-refactor-discovery.md)`
- The engine forbids unpinned closure reads (O(N²) materialization); closure = relation (pairs, directed); scc = partition (groups), not derivable without Tarjan. `(20260626.0.scc-builtin-auto-refactor-discovery.md)`

### 20260626.2 — scip-fn-level-oracle-and-coupling-sota
- CORRECTION: oracle file graph is NOT a clean DAG; 8 mutual two-cycle edges among {daemon,engine,lib,lsp,tray}; SCC builtin can't be read unpinned in derived rules (engine.rs:653 seeded-read gate). `(20260626.2.scip-fn-level-oracle-and-coupling-sota.md)`
- 0 fn-level mutual recursion (scc(scip_fn_edge) → fn_cyc_n=0): move-blockers are at file granularity (the 5-file cluster), not fn granularity. `(20260626.2.scip-fn-level-oracle-and-coupling-sota.md)`
- max() aggregate EXISTS (plan wrongly assumed a gap); naive t+1 recursion on cyclic graphs hangs forever, fixed by collapsing cyclic cluster before layering the condensation. `(20260626.2.scip-fn-level-oracle-and-coupling-sota.md)`

### 20260627.1 — from-clone-kernels-to-type-intelligence
- Precision signal discovered: proposals backed by ≥1 structural kernel (tree/cfg/cgraph/ddg) are real; proposals backed only by recall kernels (ast/ngram/symbol/verbatim) are test noise. `(20260627.1.from-clone-kernels-to-type-intelligence.md)`
- Anti-unification is the unifying technique: exact match (isomorphism), rename-erasure (alpha-equiv), and generalization (LGG) are the same operation at different thresholds. `(20260627.1.from-clone-kernels-to-type-intelligence.md)`
- closure(flow_edge) never returns in 400s (tree-wide 386k edges); node2vec closure-free but O(n²) KNN has the same cap warning (>2000 nodes). `(20260627.1.from-clone-kernels-to-type-intelligence.md)`

### 20260628.2 — openapi-flows-crosslang-nav-dl-guardrails
- references = cross-lang FOR FREE via content-addressed ref-spine string id; the codegen rhythm (operationId interned once) ties generated files together. `(20260628.2.openapi-flows-crosslang-nav-dl-guardrails.md)`
- reaches()/closure() = blast-radius engine, seeded BFS over SCC condensation (run_reaches_point engine.rs:4845), lang-agnostic, microsecond point queries. `(20260628.2.openapi-flows-crosslang-nav-dl-guardrails.md)`

### 20260629.0 — scip-impl-dispatch-go-py-tier1
- closure(calls) is REJECTED when read unpinned from a rule body (multi-source gap is real); worked around by writing reach as EXPLICIT recursion (reach(a,b)<-calls(a,b); reach(a,b)<-calls(a,m),reach(m,b)) which is freely joinable. `(20260629.0.scip-impl-dispatch-go-py-tier1.md)`
- closure() builtin = pin-one-endpoint-to-literal only; explicit recursion is the multi-source-seeding form, same pairs, freely joinable. `(20260629.0.scip-impl-dispatch-go-py-tier1.md)`

### 20260629.1 — sprefa-temporal-next-async-ghcacher
- v5 = Path B by hand (SQL fixpoint + rev + --changed + LSP-on-demand = Salsa-over-SQL with a Datomic-ish rev column); @next + @async are the named declarative form of what's already hand-rolled. `(20260629.1.sprefa-temporal-next-async-ghcacher.md)`
- @next stratifies the `p <-@next not p` paradox by construction: head lands at tx+1, negation reads tx, never the same generation. `(20260629.1.sprefa-temporal-next-async-ghcacher.md)`
- The one semantic delta is "carry reads as EDB for this tick": @next head rel must read from carry before rebuild_derived, excluded from derived_rels so the DELETE doesn't wipe it. `(20260629.1.sprefa-temporal-next-async-ghcacher.md)`

### 20260629.2 — sprefa-interproc-flow-typed-ret-dfparam
- flow_edge = df_edge UNION interprocedural hops; closure(flow_edge) walks across fns; all-derived so closure() is legal (unlike unpinned multi-source case). `(20260629.2.sprefa-interproc-flow-typed-ret-dfparam.md)`
- GOTCHA: df_node line base NOT cross-lang consistent (Rust 1-based, Kotlin/TS 0-based), call_site.line is 1-based all langs; resolve via call_edge (sym→sym, no line) instead. `(20260629.2.sprefa-interproc-flow-typed-ret-dfparam.md)`

### 20260629.3 — sprefa-node2vec-graph-embed
- node2vec embeds a node by GRAPH POSITION (walks+skipgram), orthogonal to text similar (content); two axes; concatenating them = the refactor-cluster signal. `(20260629.3.sprefa-node2vec-graph-embed.md)`
- node2vec(edge) rides the EXACT closure/scc seam: a BodyItem naming derived edge rel, excluded from rebuild_derived (can't lower to SQL), evaluated after the edge materializes. `(20260629.3.sprefa-node2vec-graph-embed.md)`
- scc/closure already solved "expensive global graph op on a reactive rule" via per-edge content digest (recondense only when rows moved); W1 (digest-skip on node2vec) copies the pattern. `(20260629.3.sprefa-node2vec-graph-embed.md)`

---



### 20260630.0 — sprefa-dl-self-validation-docs-scip-incremental
- Incremental oracle decision: "the incremental oracle is the LANGUAGE SERVER, not SCIP" (salsa, not incremental-SCIP format); for AI-parallel workload, base-OID-share + syntactic-delta > chasing incremental-SCIP (`20260630.0.sprefa-dl-self-validation-docs-scip-incremental.md`)

### 20260630.1 — dl-turnkey-ai-setup-examples-embed
- Warm path for RA LSP only: "rust-analyzer scip CLI ~11s EVERY run (cold 11.37 / 'warm' 10.59, byte-identical same-state); NO warm path for the CLI — warm RA = the LSP server" (salsa-based incremental computation) (`20260630.1.dl-turnkey-ai-setup-examples-embed.md`)

### 20260630.6 — sprefa-v3-v4-v5-research-type-edge-repo-fix
- v4 DRed: "Real incremental retraction via `SupportLedger(cursor_id,table,row_id,mult)` + DRed `cascade_retract` (mounted_query.rs:860-907): delete a row iff sum(mult)==0" (counting/weight retraction, delete-and-rederive) (`20260630.6.sprefa-v3-v4-v5-research-type-edge-repo-fix.md`)
- v4 fixpoint: "Stratified SQL fixpoint (Tarjan SCC + negation check)" (SCC, semi-naive evaluation) (`20260630.6.sprefa-v3-v4-v5-research-type-edge-repo-fix.md`)

### 20260630.7 — dl-mcp-lattice-types-paradigm-theory
- Fixed-point theory: "BOTH branches bottom out in Tarski's fixed-point theorem: monotone f on a complete lattice has a least fixed point reached by iterating from bottom. Datalog's tick() IS that staircase. merge(MaxBy) didn't add a feature, it handed the engine a RICHER LATTICE than plain-set ⊆" (fixpoint, semi-naive evaluation, lattice semantics) (`20260630.7.dl-mcp-lattice-types-paradigm-theory.md`)

### 20260702.0 — dl-perf-audit-closure-guard-one-pass-fixpoint
- v5 reactivity class: "memoized-invalidation cluster (Make/Salsa), not view-maintenance (DD)" (salsa vs differential dataflow architecture decision) (`20260702.0.dl-perf-audit-closure-guard-one-pass-fixpoint.md`)
- Dependency-graph DD exists: "DD-for-the-dependency-graph already exists in this engine: affected_derived (mod.rs:1120) is dirty-propagation over the rule graph (tens of nodes, microseconds) — DD would add machinery to save nothing. Tuple-level DD = the FactStore/support machinery v5 deliberately removed; still right." (dataflow, incremental computation, dirty propagation) (`20260702.0.dl-perf-audit-closure-guard-one-pass-fixpoint.md`)
- Stratification and SCC: "stratify() groups by NEGATION DEPTH not SCC — one stratum holds long acyclic chains" and "tarjan(adj head->body) completes dependency SCCs first => ascending comp id is dependencies-first evaluation order" (semi-naive evaluation, SCC, fixpoint) (`20260702.0.dl-perf-audit-closure-guard-one-pass-fixpoint.md`)
- Closure VIEW cost: "LIMIT does not short-circuit the closure VIEW: the top-level UNION + the recursive CTE materialize before the first row emits" (performance, fixpoint semantics) (`20260702.0.dl-perf-audit-closure-guard-one-pass-fixpoint.md`)
- Join optimization: "The unindexable-join smell: a cmp predicate computed from columns of TWO different body atoms forces per-pair evaluation over the cross product...~25M evals -> 2.5k" (semi-naive evaluation optimization, Big-O) (`20260702.0.dl-perf-audit-closure-guard-one-pass-fixpoint.md`)

### 20260702.5 — FABLE-null-pad-guard-warts-namespaces-checked-docs
- Fixpoint divergence with NULL: "NULL divergence is now MEASURED: 2^24 rows / one doubling per fixpoint iteration; NULL != NULL under INSERT OR IGNORE means delta never hits 0" (semi-naive evaluation semantics, fixpoint convergence) (`20260702.5.FABLE-null-pad-guard-warts-namespaces-checked-docs.md`)

### 20260703.0 — v041-daemon-hotreload-madge-preset-test-hygiene
- used-gate on built-in rels: "a built-in rel's table can EXIST but stay empty until some program references it (rel_module_edge empty on the live db until flow-panel.dl gained module rules)" (dataflow, incremental computation) (`20260703.0.v041-daemon-hotreload-madge-preset-test-hygiene.md`)



### 20260709.1 — fable-babysit-opus-0623-release-type-shapes-scip-typedeclrow
- React/DBSP isomorphism: reconciler at output boundary only; unmount IS retraction, inferred-by-diff vs carried-as-data (`20260709.1.fable-babysit-opus-0623-release-type-shapes-scip-typedeclrow.md`)
- Non-resident reactivity: derivation graph (tiny, persisted digests) vs memo table (SQLite); DBSP Z-set weights = row-grain retraction with zero residency; sprefa IS already the non-resident shape (`20260709.1`)

### 20260715.1 — family-derive-call-projection-flag
- MobX/SolidJS auto-tracking: dep capture dynamic (intercepted reads), deliberately NOT React useMemo (declared array = undercaptures, the alias bug) (`20260715.1.family-derive-call-projection-flag.md`)

### 20260716.0 — family-router-incremental-reconcile-cutover-debt
- Row-level incremental reconcile (retraction) via RowDelta + reconcile + retract_rows; incremental render coherent ONLY when family path is sole writer (`20260716.0.family-router-incremental-reconcile-cutover-debt.md`)
- Dep capture is computed (MobX-like), not declared: SQL reads recorded as DepKey per row; delta-driven rederive via intersection of changed vs captured deps (`20260716.0`)

### 20260716.2 — capstone-retraction-cpu-hog-kimi-delegation
- React_deltas contract returns EVERY rerun family incl. empty deltas; cold = memo-absent authoritative reload_rel (`20260716.2.capstone-retraction-cpu-hog-kimi-delegation.md`)
- Family footprint must include declared input_rels; observed-row DepKeys alone go blind on empty tables (`20260716.2`)

### 20260716.3 — daemon-sla-blitz-http-jobq-tokio
- Sync engine (CPU-bound, law applies); shell async via tokio+axum with engine sync behind spawn_blocking (`20260716.3.daemon-sla-blitz-http-jobq-tokio.md`)

### 20260716.4 — daemon-why-trail-write-storm-rootcause
- Content-digest skip at every write seam: rows_content_digest(cols, rows, scope) per-row blake3 + wrapping-add fold (order-independent, duplicate-sensitive) (`20260716.4.daemon-why-trail-write-storm-rootcause.md`)
- Honest change propagation: refresh_rel_for_revs returns bool, skips reload when digest matches + live COUNT(*) == encoded.len() (`20260716.4`)

### 20260717.0 — exe-storm-cracked-rails-and-measures
- Nondeterministic extraction unordered file-set SELECTs + cached_facts_profiled hits-then-misses, lossy-key first-wins dedup; content-based digests downstream were HONEST (`20260717.0.exe-storm-cracked-rails-and-measures.md`)
- Lossy-key dedup: many-contents→one-key, order = tiebreaker; deterministic winners via ORDER BY (`20260717.0`)

### 20260717.2 — freeze-forensics-loop-fix-chaos-soak
- Change-detection is recurring bug seam: dice-roll extraction, hardcoded Ok(true), self-measuring timers — all wrong answers to "did anything change" (`20260717.2.freeze-forensics-loop-fix-chaos-soak.md`)

### 20260717.3 — big-wins-13-14-15-arch-expr-lab-freeze-rca
- Intraproc unifier sparse by design; inter-node dataflow is relations themselves; v4's rx semantics (@next/@async/@stream/ports) mapped to v5 scan/switchMap/pipe/Subject (`20260717.3.big-wins-13-14-15-arch-expr-lab-freeze-rca.md`)

### 20260718.0 — path-analysis-rails-scip-freshness-loop-np1
- Loop detection via recursive closure + halting bound: lexical span nesting + call-step nesting + transitive loop_reach; scoped recursion via closure guard bail (`20260718.0.path-analysis-rails-scip-freshness-loop-np1.md`)
- N+1 per-row loops: loop_over.collection distinguishes per-ROW loops (changes) from benign per-REL loops (heads, async_rules) (`20260718.0`)

### 20260718.1 — planning-morning-delegation-wave-vision
- DBSP algebra representation-agnostic, DD runtime RAM-bound; IVM/DBToaster = same algebra in tables; retraction = weight -1 carried as data vs React inferring retraction by diff (`20260718.1.planning-morning-delegation-wave-vision.md`)
- Paths unfolding bills for sharing (duplication = #paths, exponential); cycles = infinite unfolding; scc condensation + layers repair; edges reify to rows (call_edge/df_edge) (`20260718.1`)
- Closure machinery once for effect coloring + lib taint + entry reach = one closure, many seeds; build closure machinery once (vision tier 3) (`20260718.1`)

### 20260718.3 — fleet-seam-instant-revival
- Async vs sync: sync engine (CPU-bound, law applies); shell async via tokio; engine sync behind spawn_blocking (`20260718.3.fleet-seam-instant-revival.md`)

### 20260718.5 — merge-chain-render-storm-arcs
- Row-level incremental reconcile mechanism (retract_rows, RowDelta, react_deltas, rail) built + proven but gated on cutover for live render (`20260718.5.merge-chain-render-storm-arcs.md`)

### 20260718.6 — phantom-diff-freeze-rca-storage-endgame
- A change-detector comparing differently-shaped encodings is constant, not detector; steady-state tests mandatory for every escalation arm (`20260718.6.phantom-diff-freeze-rca-storage-endgame.md`)

### 20260718.7 — integrator-wave-clock-efficiency-main-push
- Derived digest-before-write via TEMP mirror eval + COUNT/EXCEPT compare, skip unmark/wipe/refill/mark on identical rows; 93% of derived rels (`20260718.7.integrator-wave-clock-efficiency-main-push.md`)
- View-DDL skip: sqlite_master.sql verbatim equality exact idempotence check for DDL; clock's design premise "ticks are cheap" broke when cost 4MB WAL per tick (`20260718.7`)
- Schema DDL invisible to row-level write accounting; WAL byte measurement (checkpoint-TRUNCATE) honest probe vs _write_ledger (`20260718.7`)

### 20260719.1 — sqlite-health-command-storage-diet-eager-lazy
- Eager/lazy rel materialization design: mark derived rel "runs and saves right away" vs "don't run till reason" (demand-time, view-tier, query-time); lazy = SQL VIEW vs demand-materialized-with-eviction (`20260719.1.sqlite-health-command-storage-diet-eager-lazy.md`)
- Dense dictionary ids (1a): current ids hash-valued 8-byte StringIds defeat SQLite varint encoding; dense shrinks rows AND every index entry (`20260719.1`)

---



### 20260721.3 — v6-core-frp-edge-vs-batch-decision
- FRP pain-vs-payoff lopsided: CORE (batch-relational) = max pain/min payoff; EDGE (event-streams) = bounded pain/max payoff, the right dojo. (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)
- Pull primitive + invalidation-as-push + backpressure = Salsa; v5 already is family-memo (MobX/Solid reconciler) + rayon batch, just crude. (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)
- v5 reactivity = groupBy + immediate rerun (ultra mid); FIX stays simple: buffer(tick) -> groupBy -> auditTime -> distinctUntilChanged(digest) -> mergeMap(derive) in the trigger layer, family sync fn unchanged. (`20260721.3.v6-core-frp-edge-vs-batch-decision.md`)

### 20260721.4 — v6-sqlite-dd-dred-cycle-safe-relstore-backport
- Counting Z-set retraction correct ONLY on acyclic graphs; cycles phantom-sustain. DRed (Delete-and-Rederive) fixes it; dd correct by construction; cost ∝ wavefront cone size (no escape without demand-lazy). (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)
- Wavefront cost universal lower bound |Δoutput|; dd resident (gun wall), sqlite on disk (bounded). SQLite store config (WAL+synchronous=NORMAL+temp_store=MEMORY+one txn) honest memory vs labkit default bloat. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)
- Early-cutoff optimization: reconcile dirty() via dep-based frontier (d.changed_at > s.verified_at), NOT self-changed; verify() bumps changed_at only if digest moves, enabling lazy frontier + early termination. (`20260721.4.v6-sqlite-dd-dred-cycle-safe-relstore-backport.md`)

### 20260722.0 — v6-store-hermetic-perf-harness-dred-vs-dd-honest
- Counting ~0.44s @ 960k (fast + correct on DAGs), WRONG on cycles. DRed loop ~2.05s (5x slower, correct on cycles, cost = two passes). DRed CTE ~20% slower than loop (recursive CTE loses to index-driven temp-table frontier loop). (`20260722.0.v6-store-hermetic-perf-harness-dred-vs-dd-honest.md`)
- dd fastest ~0.17s @ 960k but resident ~215 B/node (618MB @ 2.9M). SQLite store rust_live 0.09MB FLAT (state on disk). Trade-off: speed vs. memory wall. (`20260722.0.v6-store-hermetic-perf-harness-dred-vs-dd-honest.md`)
- Hermetic = one process per engine; correctness via blake3 hash equality cross-process, never in-process; never count corpus residence; shared in-RAM graph builder masks dd's true wall at scale. (`20260722.0.v6-store-hermetic-perf-harness-dred-vs-dd-honest.md`)

### 20260721.1 — v6-store-sqlite-cascade-ivm-lab
- Weight cascade delta-proportional (work ∝ change); WITH RECURSIVE recompute path-oriented (720x slower on 16k, timeout 50k+, path explosion on fan-in). Batch-node vs recompute-path is the fundamental tradeoff. (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)
- IVM on-disk SQLite weight-cascade Feldera-style Z-set retraction bounded-memory vs resident dd/dbsp; wall proven (1.5GB budget → sqlite completes, dd/dbsp abort exit 134). (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)
- Soft-delete + transition guard (weight>0 = alive) removes per-node DELETE; survivor-set ≡ reachability-from-surviving-roots (proven), but DELTA (who dies) needs COUNT aggregation which recursive CTE forbids → CTE can only recompute. (`20260721.1.v6-store-sqlite-cascade-ivm-lab.md`)

### 20260720.0 — v6-schema-design-graph-lib-empirical-labs
- CTE halt-predicate = WHERE clause on recursive term, sub-millisecond on 261k-edge table vs byte-for-byte v5 rules; SCC/build_condensed/count_pairs stay in Rust (iterative tarjan survives 283k nodes where petgraph tarjan crashes at 74k). (`20260720.0.v6-schema-design-graph-lib-empirical-labs.md`)
- Dedup key decides termination: depth-deduped CTE grows unbound (12.8k rows by depth 200), node-deduped converges 6ms; code graphs cyclic so depth column is termination hazard. (`20260720.0.v6-schema-design-graph-lib-empirical-labs.md`)
- Storage autopsy: indexes 481MB of 853MB (56%); salt_rev (format string + mint_sym) 63% of _strings bytes; df_node in 4 projections = 163.6MB for one fact set; real graph 283k nodes/261k edges linear growth. (`20260720.0.v6-schema-design-graph-lib-empirical-labs.md`)

### 20260720.1 — graph-lib-adversarial-rounds-v6-decision
- Memory layout wins, not library: storage held constant, petgraph 1.16-1.43x SLOWER. Build cost 4.2-27.7x algo time; crossover span 1.5-297 queries per rel. (`20260720.1.graph-lib-adversarial-rounds-v6-decision.md`)
- Sampling error: rel_df_edge (friendly, max SCC 6, mean reach 5.35) vs rel_flow_edge (unfriendly, 22k-node SCC, mean reach 15.6k). Generalization from one instance is hypothesis not fact. (`20260720.1.graph-lib-adversarial-rounds-v6-decision.md`)
- SCC in SQL quadratic in peel-core size; petgraph recursive tarjan crashes at 74.5k nodes; GraphBLAS depth-fatal (158.4s/depth-4000) but size-cheap; LadybugDB SILENTLY WRONG (scc merges vertices one-direction). (`20260720.1.graph-lib-adversarial-rounds-v6-decision.md`)

### 20260721.2 — v6-cascade-tuning-ram-audit-bigO-core-crate
- Retract O(delta * log n_hot) ~= O(delta); 2x delta → 2x time (LINEAR); 5x corpus fixed delta → FLAT 270→273ms (untouched corpus costs nothing). Setup O(n). (`20260721.2.v6-cascade-tuning-ram-audit-bigO-core-crate.md`)
- Retract at SQLite floor ~1.44s, CPU-bound: weight UPDATE ~631ms (1M scattered WAL rowid rewrites) + cx_hits GROUP BY ~549ms sort. ~5x off resident dd/dbsp (~291ms) = durable on-disk-state price that wins memory wall. (`20260721.2.v6-cascade-tuning-ram-audit-bigO-core-crate.md`)
- RAM audit: DL_MEMCAP_MB bounds Rust heap only; SQLite C-heap invisible to memcap. Added libsqlite3-sys, DL_SQLITE_HEAP_MB → PRAGMA hard_heap_limit. Retract C-heap 54.5MB wavefront-bounded not corpus (38→44→54 @ 1M→5M nodes). (`20260721.2.v6-cascade-tuning-ram-audit-bigO-core-crate.md`)


