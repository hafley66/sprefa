---
name: project_node2vec_graph_embed
description: node2vec(edge) graph-embedding operator internal to dl (no shell-out); node_sim rel; reactive-perf plan W1-W5 incl static recompute-guard rail
metadata: 
  node_type: memory
  type: project
  originSessionId: a6a2eb6e-f477-44b1-97e7-8fd0568577f0
---

Internal graph embeddings in v5 dl, the GRAPH sibling of the text `similar` rel.
COMMITTED on main 2026-06-29: c3e7bf2 (feature), c17e471 (W1+W5), b6edc6a (W2).
W1 digest-skip, W5 recompute-guard rail, W2 vector cache all LANDED + green
(lib 163/0/1, it 314/0/3). W3 switchMap async + W4 warm-start still DEFERRED.

**The capability**: `node_sim(a, b, score) <- node2vec(g).` where `g` is any 2-col
dl edge rel (call_edge, flow_edge, type_link, ...). Embeds each node by GRAPH
POSITION (random walks + skip-gram), orthogonal to `similar` (text content). Two
axes; concatenate = refactor-cluster signal. Runs on real v5 call graph: 3156
node_sim rows, neighbors structurally meaningful (Rule.closure_edge ~ closure_map
0.914). NO shell-out (the user's hard constraint — keep embeddings/experiments in
dl, not piped to python).

**Files**: src/embed/node2vec.rs (DeepWalk: SplitMix64 dep-free RNG, build_adj
CSR, uniform walks [p/q seam at 1.0], negative-sampling skipgram SGD, no linalg
dep; N2vConfig::from_env SPREFA_N2V_*). Wiring rides the EXACT closure/scc seam:
BodyItem::Node2vec{rel} (ast/parse/frontend:257/typecheck:233), excluded from
rebuild_derived (can't lower to SQL), eval_node2vec_rule (engine.rs ~4758) runs
after the edge materializes. _node_embeddings(node,graph,dim,vec) table.
knn_rows(pool,k) free fn EXTRACTED as the shared cosine top-k (similar + node_sim;
wire sqlite-vec ANN once = both). node_sim is USER-named (not reserved) like
reach/scc heads. examples/node2vec-callgraph.dl, tests/it/node2vec.rs.

**THE PERF DEBT**: eval_node2vec_rule recomputes the whole embed unconditionally,
synchronously, under daemon eng.lock(); node2vec is GLOBAL (not cheaply
incremental); every git checkout re-ticks -> full re-embed. Plan:
plans/2026-06-29-node2vec-reactive-perf.md (zoom-level-2, W1-W5):
- W1 digest-skip (DO FIRST, ~25 lines): blake3 XOR-fold edge rows -> _reldigest
  "node2vec:<edge>", skip when unchanged. MIRRORS scc/closure ConditionCache.digest
  recondense-guard (engine.rs:1214/1269). Most checkouts don't move the graph.
- W2 recent-digest vector cache (_node_embeddings keyed by edge_digest + LRU) for
  branch A<->B thrash.
- W3 switchMap async tier (daemon, gate SPREFA_N2V_ASYNC): embed on a worker OFF
  the eng lock, serve last-good meanwhile. MUST be switchMap-CANCELLABLE
  (user requirement): gen:AtomicU64 per graph change; worker checks gen between
  epochs (cooperative cancel) + drain re-checks gen (only latest commits).
  One-shot CLI stays synchronous.
- W4 warm-start incremental training: DEFERRED, measured-need only.
- W5 STATIC recompute-guard rail (user's idea): catch the anti-pattern at CODING
  time, not just runtime. Marker convention in CLAUDE.md ("// @recompute unguarded:
  <reason>" or a digest guard) + dl rail examples/recompute-guard.dl (sg/ast: fn
  calls embed_graph without load_rel_digest -> diag -> --check exit 2, Claude
  blocking-hook). First row flagged = today's eval_node2vec_rule; W1 clears it.
  Start fn-level not loop-level (false-positive). Same self-host move as
  lint-imports + [[feedback_never_edit_autogen_zones]] no-touch guard.

GOTCHA: stale release binary again — target/release/dl predated the new rel;
cargo build --release before running examples after engine edits.

Related: [[project_interproc_flow]] (the flow rels node2vec can embed),
[[reference_scip_name_not_dl_split]] (sym-space node ids), [[project_v5_dl_engine]].
