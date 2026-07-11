# C4 go/no-go: 3D isometric BOM structure view

Design sketch only, no implementation. Scope: track C item C4 from
`plans/2026-07-10-vscode-ext-review.md`, evaluated against the shipped C3
exploded stratum view and C2 rollup/where-used overlay in
`editors/vscode-dl/media/flow-panel.html`, over `.dl/bom.dl`'s
`bom_node`/`bom_edge`/`bom_tier`/`bom_weld`.

## What a 3D iso view would add over the 2D exploded view

The exploded list (C3) already does the thing a BOM view exists to do: group
parts by dependency stratum (tier), band-header each stratum, and let a weld
cluster read as one row. Depth-as-concept is already represented — as a
list section, not as literal Z.

What 3D iso adds on top:

- **Simultaneous cross-stratum silhouette.** The list shows one stratum's
  members at a time (scrolled); iso would let you see 3-8 stacked planes at
  once, so "how tall is the dependency stack under this part" is a glance,
  not a scroll-and-count.
- **Occlusion as a signal.** A part with heavy fan-in sitting behind several
  planes reads as "buried" without reading a number. This is real
  information the numeric fan-in column also carries, redundantly.
- **Spatial memory for return visits.** Users who alt-click through where-used
  overlays repeatedly could build a mental map ("the auth stuff is back-left")
  faster than re-reading file paths. Unverified — no evidence this repo's
  users work that way today.

What it does **not** add:

- No new queryable fact. Every value (member count, fan-in, fan-out, weight)
  already renders as the numeric band; iso is a re-skin of geometry the list
  already encodes as rows and indentation.
- No new interaction. Hover/pin/open/where-used are all already wired to
  `bom_node`/`bom_edge` rows; iso would need to re-target the same three
  handlers at a different hit-test surface, not add capability.
- Collapse/rollup (C2) is fundamentally a row operation (fold N rows into 1,
  show a subtree total). A stack of 3D planes collapsing is a much harder
  layout problem than deleting rows from a list, for the same rollup number.

Honest read: the 2D exploded view **already captures the stratum-depth idea**.
3D iso's marginal gain is glanceability and occlusion-as-signal, on top of a
view that already works. This is a real but narrow gain, not a new capability.

## Candidate render approaches

| Approach | New dep | CSP | Bundle cost | Maintenance | Fit with existing DOM/cy |
|---|---|---|---|---|---|
| CSS 3D transforms on existing DOM (per-stratum band as a `transform: translateZ()` layer, fixed 30° / 2:1 iso projection) | none | trivially satisfied — no `<script>`, inline `<style>`, existing DOM nodes | ~0 (a few dozen lines of CSS + transform math already sketched in the plan) | low — same list rows, same event delegation, same virtualization; add a wrapper transform | best — list rows keep their real DOM identity, hover/click handlers untouched, virtualized rendering (A6) untouched since only the CONTAINER gets a 3D transform, not per-row 3D math |
| Canvas-drawn iso projection (draw one 2D `<canvas>`, hand-roll the iso math, no library) | none | satisfied — canvas drawing is inline JS, no external fetch | small (a projection + hit-test module, maybe 200-400 lines) | medium — reimplements hit-testing, hover, and hit-region bookkeeping that DOM already gives for free; cytoscape's existing canvas (renderCanvas) would need a SECOND canvas or a merged one | requires a parallel hit-test layer since canvas has no DOM nodes to delegate events to; duplicates work cytoscape already does for the 2D graph canvas |
| Vendored three.js (WebGL scene graph, orbit controls) | ~600KB+ minified, vendored into the extension bundle | satisfiable (no CDN, ship the file) but adds a large vendored blob to audit and update by hand on every security patch | large — WebGL context, scene/camera/renderer boilerplate, its own hit-testing (raycasting) to bridge back to `dlHost` open/hover | high — a whole rendering paradigm alongside cytoscape's canvas and the virtualized list; three separate render surfaces to keep in sync with one `lastKeptNodes`/`lastListMergedEdges` state | worst fit — orbit + WebGL is explicitly the kind of interaction the current fixed-projection ask does NOT want, and it's the heaviest dependency for the least fit |

CSS 3D transforms is the only approach that reuses the existing row DOM,
existing event handlers, and the existing virtualization untouched — it adds
a transform to the *container*, not new geometry per row. Canvas-drawn iso
is next cheapest but throws away DOM-native hit-testing for no material gain
over CSS transforms, since the projection is fixed (no orbit) either way.
Three.js is disproportionate to a fixed 30°/no-orbit projection and is the
only option that risks the portability law (a rendering paradigm coupled to
a specific 3D library, versus the panel's current "just DOM + a cy canvas").

## Interaction model (if built)

Maps directly onto the existing handlers — this is the strongest argument
FOR CSS transforms specifically, since nothing below needs new plumbing:

- **Hover**: unchanged. Each stratum band is a normal DOM subtree with
  `transform: translateZ(bandIndex * DEPTH)` on the band wrapper only; a row
  inside it is still a real DOM node, so `mouseover` → hover card wiring is
  untouched.
- **Pin / open**: unchanged. Click still resolves to the same row `sym`;
  `window.dlHost.open(...)` fires from the same handler, oblivious to the
  parent transform.
- **Where-used overlay (C2 alt-click)**: unchanged targeting (still keyed by
  `sym`), but the DRAWN overlay (today: highlighted rows/edges in the flat
  list) would need edge lines that cross strata to be drawn in the same
  projected space — this is the one piece of new geometry work, since an
  edge from stratum 2 to stratum 5 must be projected consistently with the
  bands it's crossing. Cytoscape's existing edge-drawing code doesn't know
  about Z; this would need a small companion path (SVG line with iso-
  projected endpoints), not cytoscape itself.
- **Collapse/rollup (C2 applyCollapse)**: rollup stays a row-count operation
  ("this collapsed band represents N rows/subtree total"); 3D only changes
  where that collapsed band's DOM sits, not the math.

## Recommendation: NO-GO for now

Strongest reason: **the exploded 2D view already delivers the concept iso
would add (stratum depth as a first-class grouping), and the cheapest 3D
approach (CSS transforms) buys glanceability/occlusion at real edge-drawing
cost (cross-stratum where-used lines need a projected-space companion
renderer) for a gain that is unverified against actual usage** — no evidence
yet that users read the exploded view's strata via scrolling and want them
simultaneously visible instead. Spend the effort on measuring how the
exploded view is actually used first.

This does not rule out CSS transforms later — of the three candidates it is
clearly the only one consistent with the portability law and CSP, and the
staged estimate below is what "later" would cost if the exploded view proves
the demand.

### Cheaper 2.5D affordance covering ~80% of the value

Ship inside the existing exploded list, no new render surface:

- **Depth-cue shading**: tint each stratum band header with a fixed
  lightness ramp by tier index (foundations darker/further, entry points
  lighter/closer) — the "closer strata are visually forward" cue without
  any geometry change. Pure CSS on the existing band header.
- **Parallax on scroll**: a subtle `translateX` or opacity delta per band as
  the list scrolls, so adjacent strata feel layered without leaving list
  layout. Cheap CSS/JS, no hit-test changes, reversible in one commit.
- **Sticky mini-map**: a fixed-position column of small stratum-index chips
  (already have tier numbers from `bom_tier`) that highlights the strata
  currently in viewport — answers "how tall is the stack, where am I in it"
  without any 3D projection at all.

All three are pure CSS/list-scroll additions, zero new dependencies, zero
new hit-testing, and directly attack the "simultaneous cross-stratum
silhouette" gap identified above as iso's one genuine advantage.

## If GO later: staged estimate

Contingent on the exploded view showing real stratum-scrolling pain first.

- **Stage 1 (S)**: CSS transform container + fixed 30°/2:1 projection over
  the exploded view's existing band structure; hover/pin/open verified
  unchanged; toggle to fall back to flat exploded view.
- **Stage 2 (S-M)**: cross-stratum where-used edge lines in projected space
  (the one new geometry surface identified above); reuses `bom_edge` data,
  new small SVG-overlay path only.
- **Stage 3 (S)**: perf validation at the 2000-node cap (per-band DOM count
  under one frame budget) and self-occlusion cutoff (auto-collapse or warn
  past ~12 strata, per the plan's own go/no-go condition).

Total ~M, gated on Stage 1's real usage signal before Stage 2 is scheduled.
