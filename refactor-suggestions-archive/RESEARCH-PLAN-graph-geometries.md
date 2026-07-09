# Research plan: code-graph geometries of refactor candidates

## 0. Thesis
A refactor candidate type is a **recurring geometric motif in the code relation graph**. The goal is a
**deterministic detector** — graph queries over sprefa's own extracted relations — that enumerates
those motifs and emits candidate sites with a ranked score. **No LLM in the detection path.** The
roadmap adds statistics/probabilities, then embeddings, for fuzzy matching the exact queries miss —
still deterministic at inference.

The agent ensemble is **not the other half**. It is a ratchet: a *source* of motif patterns to
formalize, and a *validation oracle* (the Σ/12 consensus = silver labels to score the detector). It is
never in the loop and never required at runtime.

## 1. This already started — `propose.rs` is detector v0
We are on branch `propose-extract`. `propose.rs` is a deterministic clone detector: structural-hash
kernels (`ast_shape`, `subtree_hash`, `cfg_skeleton_hash`, `ddg_hash`, `callgraph_hash`, `ngram_set`,
`symbol_shape`, `leaf_kinds`) + `matching_runs`/`similarity_runs` + a `gain`/feasibility ranker. That
**is** the "isomorphic fan" motif detector. The research generalizes it: more motifs, grounded in the
existing relations, with the agent consensus as the recall check.

## 2. The substrate (what the detector runs on)
sprefa already extracts these relations — the detector is queries over them, expressible as `.dl`
rules or plain Rust graph passes:

| Relation | Shape | Motif use |
|----------|-------|-----------|
| `node(id,kind,file,lo,hi,parent)` + nested-set `child`/ancestry | CST | structural hashing, subtree iso, conductance |
| `type_entity(sym,name,kind,parent,file,line)` | decls | sibling grouping, fan |
| `type_edge(from,to,kind)` / `type_link(src,dst,kind)` | type graph | quotient, fan-in/out |
| `type_sig(sym,slot,pos,ref)` | arrow/params exploded | co-travel (arg tuples) |
| `module_edge`/`module_import`/`crate_edge` | module graph | conductance, layering |
| `call` relations / `scip_edge` | call graph | seam-leak, callgraph hash |
| `df_node`/`df_edge`/`loop_fact` | dataflow | co-travel (co-constructed/co-read) |
| `ref(id,string,file,lo,hi)` + `string(id,text,norm)` | located literals | shared-discriminator (repeated keys) |

Dogfood: the detector analyzes sprefa's own source via sprefa. Self-application is the demo and the
first corpus.

## 3. The motif dictionary (the graphs to enumerate — current phase)
Each motif = a deterministic signature over §2 relations → candidate sites + a score. Closing this
table is the work. A motif earns a row only if it maps to ≥1 candidate the ensemble already surfaced
(bounds the enumeration).

| Motif | Deterministic signature | Score | Ensemble candidates (silver) | Detector status |
|-------|------------------------|-------|------------------------------|-----------------|
| **Isomorphic fan** | group items by structural hash (ids/literals normalized); flag hash-classes with count ≥ K under sibling scope | K × node-size × (occ−1) | C1, C6, C10 (Σ=12,12,8) | **exists** (propose kernels) |
| **Twin path** | two items, structural edit-distance = 1 (differ by one node/leaf) | similarity × size | C5 tick/tick_paths, entries/entries_kd, broadcast_* | partial (`similarity_runs`, O(n³) → needs LSH) |
| **Co-travel set** | a var/field tuple whose members are co-constructed and co-passed/co-read across ≥ K sites (join `df_edge` / `type_sig` slots / arg lists) | K × tuple-arity | C2 (`by_name,sym_at,scip`), C9, C13 | **new** |
| **Shared discriminator** | a `string`/`norm` literal appearing as a decision key (match-arm / `==`) at ≥ K sites with a closed value set | K × distinct-sites | C4 edge kinds, C16 `Fmt` | **new** |
| **Relabel quotient** | K subgraphs with identical structural hash **across files/languages** modulo leaf-vocab (cross-file clone) | K × size | C8 flow_kt/ts/rs walkers | near (ast_shape normalizes ids; extend to cross-file) |
| **Low-conductance node** | per file/fn: (cross-boundary edges)² / (internal edges) high ⇒ god node; cut = the sparse boundary | conductance ratio | engine.rs split, C18 `handle_request` | **new** (module_edge + node counts) |
| **Seam leak** | call/use edges from layer A to a banned target T (e.g. `.conn()`), counted, where a chokepoint API exists | leak count | C12 `.conn()` reads | **new** (call-graph query) |

Open enumeration questions (the part you're mid-way through):
- Is **twin path** a special case of isomorphic fan (K=2, edit-distance≤1) or its own motif? (affects the detector's grouping key.)
- Does **co-travel** need real dataflow (`df_edge`) or does syntactic "always-adjacent in arg lists / struct-init" suffice and stay cheaper?
- **Conductance** needs a layer labeling — derive from module graph automatically, or seed a few layer labels by hand?

## 4. Detector design (deterministic, exact-first)
- **Structural hash:** reuse propose's normalized AST hash; canonical form = node-kind tree with
  ids→`ID`, literals→`LIT` (already done). Cross-file iso = same hash, different `file`.
- **Grouping:** hash → class; class size = occurrence count; rank by `gain = size × node_count × (occ−1)`
  (propose already computes this; unify the lines-vs-tokens bug noted in STATS §4).
- **Co-travel / discriminator / seam:** relational joins, no hashing — pure datalog/SQL over §2.
- **Conductance:** graph pass over `module_edge`/`type_edge` adjacency; standard sparsest-cut heuristic.
- **Output:** `candidate(motif, sites[], score, suggested_shape)` where `suggested_shape` is a pure
  function of motif (fan→table, co-travel→struct, discriminator→enum+trait, quotient→trait, conductance→file-cut, seam→chokepoint).

## 5. The ratchet (bounded role of agents)
Three uses, all offline, none in the detection path:
1. **Seed:** ensemble runs surface motif candidates a human formalizes into §3 rows.
2. **Validate:** score the detector against silver labels (Σ/12 ≥ τ sites) — recall (did the detector
   find the 12/12 motifs?) and precision (are its extra hits real?). Spot-verify a sample to gold.
3. **Regression ratchet:** re-run occasionally; if the detector drifts off a motif the ensemble keeps
   naming, that's a detector gap to fix. The ensemble is the slowly-moving truth, the detector chases it.

Validation metric: detector-sites vs silver-sites → P/R/F1 at τ = Σ/12 ≥ 6 (Strong tier). Target: the
isomorphic-fan detector reproduces the 12/12 candidates (C1, C6) at recall 1.0 before adding any motif.

## 6. Roadmap (the "till we add stats/embeddings" sequence)
- **Phase A — exact deterministic.** Boolean/count thresholds. Generalize propose kernels; add
  co-travel, discriminator, seam, conductance as relational/graph queries. Validate vs silver. **No
  probabilities, no LLM.**
- **Phase B — statistical / probabilistic.** Frequency significance instead of fixed K; MinHash/LSH to
  make twin-path/near-clone tractable (kills propose's O(n³) `similarity_runs`); rank by calibrated
  gain. Thresholds learned from the silver labels, not an LLM.
- **Phase C — embeddings.** Structural/code embeddings of subtrees for **fuzzy** motif matching the
  exact hash misses (renamed-but-equivalent, reordered). Deterministic at inference (a fixed encoder,
  nearest-neighbor in vector space). Still no LLM in the loop.

## 7. Threats to validity
- Silver labels could share model bias → keep a hand-gold sample; treat consensus as *recall floor*,
  not ground truth (the real bugs were low-consensus).
- Single repo → add 1–2 external Rust repos once Phase A passes on sprefa.
- Structural hash over-merges (false fan) / under-merges (misses renamed clones) → Phase C is the
  designed fix; measure the gap in Phase A so C has a target.
- Conductance needs a layer labeling that may not be canonical → derive + report sensitivity.

## 8. Immediate next steps
1. **Freeze §3** — stop enumerating when every Strong-tier silver candidate has a motif and every motif
   has a candidate. Resolve the three open questions in §3.
2. **Phase A, motif 1:** point the existing propose hash at *all* sibling-scope items (not just
   intra-fn statement clones) and check it reproduces C1's 14-`*_rel_decls` fan and C6's 4 refreshers.
   That single experiment answers "does the deterministic detector reproduce 12/12 consensus."
3. **Add the co-travel detector** (relational join over `type_sig`/arg lists) — the highest-value motif
   that propose does *not* already cover, and the one that yields `NameResolver`-shaped structs.
4. Decide the substrate: **`.dl` rules** (dogfood, reactive, free incremental) vs Rust graph passes.
   Recommend `.dl` for the relational motifs (co-travel/discriminator/seam) and Rust for the hashing +
   conductance.
