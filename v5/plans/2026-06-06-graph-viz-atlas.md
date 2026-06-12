# Graph-Viz / Exploration Layer — Atlas + Build Plan

Source: deep-research run 2026-06-06 (23 sources, 23/23 claims passed 3-vote adversarial
verify, 2 myths killed). Two layers throughout: (A) general technique, (B) sprefa note.

Anchor data today: `dl … --query-json` over `type_edge(from,to,kind)`, `module_edge`,
ref byte-span spine. Live demos in `/tmp/typeviz/` (d2/ELK schematic, Cytoscape, 3d-force-graph).

---

## Three settled rules (the doctrine)

1. **Viz alone fails.** Effective graph analysis = visual presentation **+** interaction **+**
   algorithmic analysis, combined (von Landesberger et al., CGF 2011). Build all three, not just pictures.
2. **Local centrality is per-tick; global is batch.** Degree / fan-in-out = O(1) per edge, fits the
   reactive tick. Betweenness / closeness / eigenvector / PageRank need the whole graph → batch only.
3. **Matrix beats node-link as density grows** (proven above ~20 nodes, Ghoniem/Fekete/Castagliola;
   Alper et al. for comparison tasks). Exception: path-following, where node-link wins.

Two myths the verifier KILLED:
- "Leiden guarantees all communities connected" — false; it guarantees *well-connected* (weaker).
- A specific naive-betweenness O(m³) figure — refuted; use Brandes O(nm) as the real cost.

---

## Atlas (technique → sprefa applicability)

### 1. Representations by dimension

| Dim | Technique | sprefa note |
|-----|-----------|-------------|
| 2D | Node-link, **Sugiyama/layered**, orthogonal | Have it (d2/ELK). Code graphs are mostly DAGs → layered is the right default. |
| 2D | **Adjacency-matrix** view | Add for dense `type_edge`/`module_edge` and rev-to-rev diff. Beats node-link as it densifies. |
| 2D | Arc diagrams, hive plots, treemap/icicle, edge bundling | Treemap/icicle for module *containment*; bundling only when hairball. Lower priority. |
| 2.5D | Layered Z-planes, stacked maps | Helps ONLY when Z is categorical+ordered (SCC depth, kind). Continuous-Z force blobs read worse than honest 2D. |
| 3D | Force-directed 3D (3d-force-graph) | Have it. Use its **DAG mode** (`dagMode`: td/lr/radialout…) — free layered 3D you're not using. Exploratory overview, not primary analysis. |
| 4D | **Dynamic graphs**: animation (time→time) vs **timeline** (time→space) | For `*_rev` relations. Pick **timeline/small-multiples** over animation — preserves mental map (Beck et al., CGF 2017). |
| nD | **Multilayer/multiplex** | type_edge / module_edge / ref as co-existing **layers** over shared nodes (McGee et al., CGF 2019). Caveat: ref-spine (byte-span) nodes aren't strictly the same set as type/module nodes. |
| nD | Embeddings projected down (UMAP/t-SNE) | Deferred — see §4. |

### 2. Orderings (nodes + edges)

| Technique | sprefa note |
|-----------|-------------|
| **Seriation** (6 families: Robinsonian, Spectral, Dimension-Reduction, Heuristic, Graph-Theoretic, Bi-Clustering — Behrisch STAR, CGF 2016) | The order that makes a matrix view legible. Start **barycenter/spectral/optimal-leaf-ordering**. Ref impl: R `seriation` pkg. |
| **Topological sort / layering / ranking** | Already implicit in ELK layered. Use for the matrix row/col order on DAG parts. |
| **SCC condensation** | You compute SCCs already (`closure`) — condense cycles to super-nodes before layout. |
| Edge ordering (bundling, arc) | Only if edge clutter forces it. |

### 3. Grouping / clustering

| Technique | sprefa note |
|-----------|-------------|
| **Leiden** | Default auto-grouping. NOT Louvain (Louvain yields arbitrarily-bad / disconnected communities, esp. iterated — Traag 2019, up to 23% bad). |
| Infomap, label propagation | Alternatives; Leiden is the safe first pick. |
| **SCC / k-core** | Cheap structural grouping, already in reach. Collapsible overlays. |
| Hierarchical clustering, SBM, coarsening | For scale / nested views. Later. |
| Manual / semantic grouping | By crate / module / path prefix — you have the data. Cheapest grouping of all. |

### 4. Statistical / analytic

| Technique | Cost | sprefa note |
|-----------|------|-------------|
| **Degree / fan-in-out** | O(1)/edge | **Per-tick.** Node sizing/color in Phase 1. |
| Betweenness / closeness / eigenvector / PageRank | global pass | **Batch only**, on demand. "Bridge" / importance ranking. |
| Density, diameter, clustering coeff, assortativity | global | Batch summary stats per relation. |
| **Motif / graphlet counting** | #P-hard | Expensive batch analytic; subgraph counting is the core cost. Not per-tick. |
| **Embeddings** (node2vec / GraphSAGE) + UMAP/t-SNE | batch+train | Deferred path to similarity + 2D/3D layout over type/module nodes. |
| Graph signal processing (GFT/Laplacian) | theory | Frontier, not a near-term build target. |

### 5. Sauce (verified projects)

| Project | What | Status / license |
|---------|------|------------------|
| **OGDF** | C++ layout (Sugiyama, orthogonal, planar) | self-contained; **GPL** |
| **ELK / elkjs** | Java layout, JS via GWT | you use it; elkjs lives at `kieler/elkjs` |
| **3d-force-graph** | ThreeJS 3D, d3-force-3d/ngraph, **DAG mode** | actively maintained (vasturiano) |
| **CDlib** | one API over many community-detection algos | `GiulioRossetti/cdlib`, maintained |
| **igraph** / leidenalg | core algorithms + Leiden | maintained |
| **emerge** | **code-as-graph visualizer** — closest cousin to sprefa | `glato/emerge` — study it |
| R **seriation** | the 6-family matrix-reordering reference impl | CRAN, maintained |

---

## Build plan (phased, leverage-over-cost)

**Phase 1 — Foundation (start here).**
2D node-link over `type_edge`/`module_edge` via existing d2/ELK layered layout +
incremental **local centrality** (degree/fan-in-out) as node size/color. Interaction:
click-to-focus + neighborhood expand. Delivers the three pillars at lowest cost.

**Phase 2 — Order + density.**
Adjacency-matrix view for dense regions and rev-to-rev comparison; **seriation**
(barycenter/spectral/OLO) to reveal block structure; SCC condensation + topo order for layering.

**Phase 3 — Grouping.**
Leiden clustering + SCC/k-core overlays as collapsible groups; graph coarsening for scale.

**Phase 4 — Multilayer + time.**
type_edge / module_edge / ref as multiplex layers; rev-aware **timeline / small-multiples**
(not animation) for WORK-vs-HEAD diffs.

**Phase 5 — Deferred (batch-only, on demand).**
Global centrality (betweenness/closeness/PageRank), embeddings + UMAP/t-SNE, motif/graphlet
counting, GSP. 3D stays exploratory.

Sequencing is synthesis, not a cited result — validate against real graph sizes + LSP debt.

---

## Open questions (need sprefa data, not the web)

1. **Row counts per relation** on the kernel checkout → decides node-link vs mandatory matrix, and whether global centrality is affordable at all.
   - ANSWERED for `v5/src` (2026-06-06): `type_edge`=185, `module_edge`=41. Both tiny → node-link viable, matrix not yet required, global centrality trivially affordable. Kernel (133MB) still untested — that's the stress case.
2. **Which seriation algo** wins on sparse directed DAG code-matrices specifically (STAR is app-agnostic).
3. **Incremental Leiden under the tick model?** Or clustering is batch-only like motif counting.
4. **New layout backend vs extend ELK** — weigh against LSP maintenance debt.

---

## Phase 1 — concrete task list (against real `--query-json`)

- [ ] **T1. Centrality query.** `degree(node, k)` / `fan_in(node,k)` / `fan_out(node,k)` as
      derived relations over `type_edge` (and `module_edge`). Verify counts vs `/tmp/tedge.jsonl`
      (185 edges, 70 from-types; top fan-out today: Tok=21, BodyItem=9, Engine=9).
- [ ] **T2. Export shape.** One `--query-json` → JSON the viewer reads directly:
      `{nodes:[{id,deg,fan_in,fan_out,kind?}], links:[{source,target,kind}]}`. Generalize
      `/tmp/typeviz/gen.py` to read this instead of recomputing degree in JS.
- [ ] **T3. Layered default.** Switch the Cytoscape viewer's default layout to ELK `layered`
      (DAG), keep fcose as the toggle. Wire 3d-force-graph `dagMode:'lr'`.
- [ ] **T4. Node encoding.** Size = degree, color = kind, border = SCC membership.
- [ ] **T5. Interaction.** Click a node → highlight `closure(type_edge)` blast radius
      (you already have the reaches query). Neighborhood expand/collapse.
- [ ] **T6. SCC condense toggle.** Collapse each SCC to a super-node (data already available).

Demos: `/tmp/typeviz/gen.sh` rebuilds + opens all four artifacts.
