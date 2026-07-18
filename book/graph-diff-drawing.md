# Drawing the change: graph differential visualization

A reading survey for one wish: *see the type graph or flow graph change* — the
diff itself as a picture, with the largest structures rendered as low-elevation
background terrain and the change on top. The literature splits the wish into
three solved-ish problems: keeping stable nodes still (the mental map), drawing
the delta (difference maps), and rendering mass as landscape (density fields).
This page is the curated primary-source list, then a per-language tool
inventory for actually building it.

## The three problems and their canonical papers

### 1. Stable nodes stay put — "mental map preservation"

| Paper | Link | Why read it |
|---|---|---|
| Misue, Eades, Lai, Sugiyama, *Layout Adjustment and the Mental Map* (1995) | [PDF](https://www.cs.tsukuba.ac.jp/~misue/misue-publications/techreport/isis-rr-94-6e.pdf) | Defined the problem; every later paper cites it |
| Diehl, Görg, *Graphs, They Are Changing* (GD 2002) | [DOI](https://doi.org/10.1007/3-540-36151-0_3) (paywalled) | "Foresighted layout": lay out the whole snapshot sequence at once so no frame surprises you |
| Beck, Burch, Diehl, Weiskopf, *The State of the Art in Visualizing Dynamic Graphs* (EuroVis STAR 2014) | [PDF](https://www.visus.uni-stuttgart.de/documentcenter/forschung/visualisierung_und_visual_analytics/eurovis14-star.pdf) · [journal version](https://onlinelibrary.wiley.com/doi/10.1111/cgf.12791) | The field map: animation vs timeline vs hybrid, taxonomy of everything below |

### 2. Drawing the delta — "difference maps"

| Paper | Link | Why read it |
|---|---|---|
| Archambault, Purchase, Pinaud, *Animation, Small Multiples, and the Effect of Mental Map Preservation in Dynamic Graphs* (TVCG 2011) | [DOI](https://doi.org/10.1109/TVCG.2011.99) | The user study: small multiples faster, animation less error-prone on some tasks, neither dominant |
| Archambault, Purchase, *Difference Map Readability for Dynamic Graphs* (GD 2010) | [PDF](https://www.labri.fr/perso/bpinaud/diffmap/DiffMapFinalVersion.pdf) | The punchline paper: union layout + color-coded added/removed/stable beats both animation and small multiples for change-reading |

The recipe they converge on: lay out the **union** of both snapshots once, then
paint membership (added / removed / persistent) as color. Position encodes
structure, color encodes time. That is the whole trick.

### 3. Mass as terrain — density fields and nested shells

| Paper | Link | Why read it |
|---|---|---|
| van Liere, de Leeuw, *GraphSplatting: Visualizing Graphs as Continuous Fields* (InfoVis 2001) | [PDF](https://homepages.cwi.nl/~robertl/papers/2001/splat/paper.pdf) | The primary source for "graph as height field": splat each vertex as a Gaussian, render contours/elevation |
| Gansner, Hu, Kobourov, *GMap: Drawing Graphs as Maps* (GD 2009) | [arXiv](https://arxiv.org/abs/0907.2585) · [gallery](https://yifanhu.net/MAPS/index.html) | Clusters as countries; the cartographic metaphor done properly |
| Alvarez-Hamelin, Dall'Asta, Barrat, Vespignani, *k-core decomposition visualization* (2005) | [arXiv](https://arxiv.org/abs/cs/0504107) · [tool](https://lanet-vi.fi.uba.ar/) | Nested shells by core number: the "largest set at lowest elevation" idea with a graph-theoretic definition of elevation |
| Collins, Penn, Carpendale, *Bubble Sets* (InfoVis 2009) | [project](https://vialab.ca/research/bubble-sets) | Isocontours over an existing layout for set membership, without moving anything |
| Meulemans et al., *KelpFusion* (TVCG 2013) | [PDF](https://pure.tue.nl/ws/files/90842832/KelpFusion_TVCG_2.pdf) | The hybrid between hulls and lines; won its user study |

### Coping with "there is just too much"

| Paper | Link | Why read it |
|---|---|---|
| Holten, *Hierarchical Edge Bundles* (TVCG 2006) | [PDF](https://www.aviz.fr/wiki/uploads/Teaching2014/bundles_infovis.pdf) | Route edges along the hierarchy; hairball becomes arteries |
| Holten, van Wijk, *Force-Directed Edge Bundling* (2009) | [TU/e record](https://research.tue.nl/en/publications/force-directed-edge-bundling-for-graph-visualization) | Bundling without needing a hierarchy |
| Hurter, Ersoy, Telea, *KDEEB: Graph Bundling by Kernel Density Estimation* (EuroVis 2012) | [project + PDF](https://webspace.science.uu.nl/~telea001/InfoVis/KDEEB) | Bundling *is* a density field: same math as the terrain layer, so one KDE pass can feed both |
| von Landesberger et al., *Visual Analysis of Large Graphs* (CGF 2011) | [DOI](https://onlinelibrary.wiley.com/doi/abs/10.1111/j.1467-8659.2011.01898.x) (paywalled) | The scale survey: aggregation, navigation, interaction at million-node size |
| Bostock, hierarchical edge bundling in D3 | [Observable](https://observablehq.com/@d3/hierarchical-edge-bundling) | Runnable reference implementation on a real dependency graph |

**Shortest path to the wish**: GraphSplatting (terrain) + the two Archambault
papers (diff recipe) + KDEEB (share the KDE pass). Union layout, KDE elevation
bands underneath, add/remove coloring on top.

## Addendum: implementations by language

What follows is a tool inventory from working knowledge (2025-era), not a
verified-links survey like the papers above; check current activity before
depending on any.

### Rust

| Crate | What it is *for* | Notes |
|---|---|---|
| `petgraph` | In-memory graph **data structures + algorithms**: Dijkstra, A*, Tarjan SCC, toposort, min spanning tree, isomorphism, dominators. The `Graph`/`StableGraph`/`GraphMap` types are the lingua franca other crates interop with | It draws nothing. Its one visual affordance is `Dot` export (emit Graphviz text). "I know petgraph but what for": it is the layer *below* drawing — you compute the union graph, the diff membership, the core numbers in petgraph, then hand positions off to something else |
| `layout-rs` / `graphviz-rust` | Layout: pure-Rust Graphviz-dot reimplementation / bindings to the real Graphviz | For when you want hierarchical (Sugiyama-style) positions without shelling out, or with |
| `fdg` (force-directed graph) | Force-directed layout over petgraph structures | Simulation only; pair with any renderer |
| `egui_graphs` | Interactive graph widget for egui, petgraph-backed | The quickest native-app path in this repo's ecosystem |
| `plotters` | General chart rendering | The canvas you rasterize a KDE field onto if you build the terrain layer natively |
| `rustworkx` | Rust graph algorithms core with Python bindings | Faster NetworkX replacement; relevant if the analysis side lives in Python |

The honest Rust summary: algorithms are excellent (petgraph), layout is
serviceable, *rendering* is DIY. The idiomatic pipeline is petgraph → layout →
serialize positions → render in a web view or egui.

### JavaScript

| Library | What it is for | Notes |
|---|---|---|
| **Cytoscape.js** | The full graph-app framework: model + styling (CSS-like selectors) + interaction + a layout ecosystem (`fcose`, `cola`, `dagre`, `elk`, `klay` as plugins) | BSD, mature, huge extension list. Diff maps fit naturally: put `added`/`removed`/`stable` as element classes and style them; `cytoscape-bubblesets` exists for the set-overlay layer. Canvas renderer; comfortable to a few thousand elements, strained past ~10k |
| ReGraph | Commercial React SDK (Cambridge Intelligence, KeyLines lineage): WebGL renderer, time bar, combos (aggregation) | The polished paid path; WebGL headroom beyond Cytoscape's canvas. License cost and closed source |
| sigma.js | WebGL renderer for large static-ish graphs (graphology model) | The open-source scale answer when Cytoscape's canvas gives out |
| d3-force / d3-contour | Low-level: simulation + KDE contouring | `d3-contour` is exactly the terrain-band primitive; composes under any renderer |
| vis-network, AntV G6 | Batteries-included alternatives | G6 is actively developed and WebGL-capable; vis-network is easy but aging |

Preference noted: Cytoscape. For the terrain background specifically, the
composition is d3-contour (density bands as an underlay canvas/SVG) with
Cytoscape's graph on top, positions shared.

### Go

| Library | What it is for | Notes |
|---|---|---|
| `gonum/graph` | The petgraph of Go: structures + algorithms (shortest path, SCC, community detection, network flow) plus DOT encode/decode | Same story as petgraph: no rendering |
| `dominikbraun/graph` | Newer generics-based graph library; friendlier API than gonum for structures | Fewer algorithms than gonum |
| `goraph` | Older algorithms collection | Mostly historical |
| Graphviz via DOT | The de facto Go rendering path: emit DOT from gonum, shell out to `dot`/`sfdp` | `sfdp` handles large graphs; Graphviz also ships GMap-style cluster maps (`gvmap`) |

The honest Go summary: analysis in gonum, rendering delegated to Graphviz or a
web frontend. No native interactive-viz story worth building on.

### Where dl sits in this

The diff side of the recipe is a Datalog query, and this engine already speaks
it: edge-set at rev A, edge-set at rev B, three derived rels
(`added_edge` / `removed_edge` / `stable_edge`) by join and negation. The flow
panel's `_node`/`_edge` convention renders any such rel pair as a toggleable
layer without a preset edit. The missing piece is only the terrain: one
per-node scalar column (component size, core number, or KDE height) and a
client-side contour pass (d3-contour) under the existing layers.
