# flow panel: arc-list view (fs-snippet with gutter edges)

Date: 2026-07-03. Status: launching T1-T4 as one Sonnet arc.

## Eval — why the canvas graph isn't the primary view

The node-card canvas (CSS anchor edges, draggable cards) hit four walls on the
first real dogfood (module graph, 58 nodes / 161 edges):

1. **Layout.** Longest-path layering + alphabetical order tangled edges badly.
   A hand-rolled barycenter pass (2026-07-03, uncommitted) improved crossings
   but crashed the webview on the live graph. A real engine (dagre/elk) means
   either bundling a library into the self-contained webview or porting
   ~/projects/anim's CssGraph trick (headless cytoscape as layout brain,
   position readback into the DOM renderer). Both are real cost, and layout
   quality is STILL the weak axis of free-canvas graphs at this density.
2. **DOM weight.** Card nodes with ports + per-edge svg divs needed
   content-visibility + measured intrinsic sizes just to idle. The default
   preset LIMITs (600/1200) are already past comfortable.
3. **Edge legibility/juice.** 161 free-form beziers cross the whole canvas;
   marching-dash animation on all of them ate the GPU (fixed by
   highlight-only animation, but the *visual* tangle remains).
4. **Readability.** User verdict: "the whole node thing is meh", "no idea how
   to use this effectively". Cards floating in space carry no orientation;
   a developer's mental model of a codebase is the file tree, not a plane.

## Decision

Build an **arc-list view** as the panel's default: a filesystem-snippet
listing where every line is secretly a node, and edges are drawn as bracket
arcs in two side gutters — **downward edges (src row above dst row) in the
left gutter, upward/back edges in the right gutter** — indented by the fs
tree (repo/rev deferred; today's presets are single-repo).

Why this shape wins, point by point against the walls above:

1. Layout engine: **deleted**, not solved. Row order = tree walk of
   (file path, line). Indent = depth. There is nothing to lay out.
2. DOM weight: fixed-height text rows in one scroll container. Browsers eat
   thousands of these; `content-visibility: auto` with an EXACT fixed
   `contain-intrinsic-size` (rows are constant-height, no measuring pass).
3. Edges: two svg gutters, one static path per merged edge, lane-allocated
   like git-log rails (interval coloring). Bundling is natural (same-(src,dst)
   dedup → stroke width; shared trunks later). Zero idle animation; hover
   animates only the touched arcs.
4. Readability: it reads as a file tree — a surface every developer already
   knows how to scan. Edges become annotation on a familiar object instead of
   the object itself.

The canvas view stays behind a `list | canvas` toggle (same queries, same
legend/filter/pin machinery). Prior art for a future canvas fix:
`~/projects/anim/src/CssGraph.ts` runs real cytoscape `headless:true` purely
for layout() and writes positions back into the DOM renderer — port that IF
the canvas earns its keep after the list view lands. Do NOT adopt cytoscape
as renderer; the DOM/CSS surface is what the host seam (hover decorations,
open-file) is built on.

## Non-goals / known issues carried

- Canvas crash on the live module graph: not root-caused; superseded (canvas
  demoted, layout pass that likely caused it is behind the toggle). Revisit
  only if canvas survives.
- Toolbar top-row buttons reported unclickable in the webview: uninvestigated,
  moot if reproduction dies with the new default view; re-check after landing.
- Dir collapse/expand + edge rollup to collapsed dirs: v2. v1 renders the
  tree fully expanded.
- repo/rev indentation levels: deferred until a preset actually returns
  multi-repo rows (data model supports it; sort key is ready for a prefix).
- Trunk-sharing (arcs sharing a lane merging into one vertical run): v2;
  v1 does (src,dst) dedup only.

## Task list

- [ ] **T1 list-view skeleton** — mode toggle (`list | canvas`, persisted,
      list default), pure `buildRows(nodeRows)`: group by file, dir structure
      rows from path segments (single-child dir chains compacted GitHub-style),
      sym rows ordered by line under their file row; a node whose id IS the
      file path becomes the file row itself (module graph case). Fixed row
      height. Row = indent + kind dot + label + kind tag. Click row = pin,
      click line/loc cell = open file. Why first: everything else hangs off
      the row index map.
- [ ] **T2 gutter arcs** — pure `assignLanes(spans)` greedy interval coloring
      (deterministic: sort by span length then row), two absolutely-positioned
      svgs (left = down edges, right = up edges) spanning full list height
      inside the same scroll container; arc = out-run-in bracket with rounded
      corners + arrowhead at dst; (src,dst) dedup → stroke width by
      log-multiplicity; kind color; lane cap with overflow lane. Why: the
      whole point — edges you can follow with a finger.
- [ ] **T3 shared-state wiring** — hover row highlights its arcs +
      counterpart rows and calls `host.hover(files)`; pins persist; legend +
      kind filter dim rows AND arcs in list mode; count pill counts as today;
      dropped-edge accounting unchanged. Why: parity so the toggle is free.
- [ ] **T4 perf rails** — zero idle animation in list mode; arcs redraw only
      on data/filter change (never on scroll); `content-visibility: auto` +
      exact intrinsic size on rows; single svg per gutter (no per-edge divs).
      Why: this view exists because the canvas was heavy — don't re-import
      the weight.
- [ ] **T5 (deferred) canvas residuals** — headless-cytoscape layout port
      from anim CssGraph; crash repro; top-button click bug. Only if canvas
      usage survives the list view.

## Verify

- `node --check` on the extracted inline script.
- `tsc -p editors/vscode-dl` still clean.
- Node harness (pure functions extracted verbatim): buildRows ordering
  (dirs nest, syms under file by line, id==path collapses to file row,
  single-child chain compaction), assignLanes (non-overlapping spans share
  lane 0, overlapping spans get distinct lanes, deterministic across runs).
- vsce package + install, manual dogfood on the module-graph preset.
