# vscode-dl review: perf remediation + navigation lens + BOM structure view

## Context

The extension (editors/vscode-dl) is useful but was accreted fast: it has "horrible
performance when it has huge things", and its navigation surface is shallow relative to what
the engine's relations can answer. Goals, per Chris: (a) evaluate usefulness/ergonomics/perf,
(b) design follow-the-user plus a way better find-all-refs/types/decls/uses incl. cross-repo,
(c) a "2D bill of materials" way of seeing program structure, with a 3D mechanical-iso
mapping sketched for later. Decisions locked: evolve the existing panel (no new webview);
2D first, 3D as a sketch rung; implementation delegated to Opus/Sonnet agents.

## Evaluation summary (grounded)

Architecture: extension.ts (586 lines, thin LSP client + fact writers + webview wrapper),
media/flow-panel.html (3,422 lines inline JS: canvas=cytoscape, list=trie+gutter-arcs,
trace), src/lsp.rs (717 lines, single-threaded loop sharing one Engine).

Perf root causes (all verified against code):
1. Single-threaded LSP loop: dl/query, hover/def/refs, and didSave -> tick_paths serialize
   on one Engine (lsp.rs:156-206). Every dl/query also does two writes (log_query +
   refresh_query_log, lsp.rs:399-400) before the read.
2. Triple row-cap waste: engine returns ALL rows -> host caps 20,000 (extension.ts:567) ->
   webview slices to 2,000/4,000 (flow-panel.html:3046). Up to 18k rows of dead serialize.
3. List view not virtualized: innerHTML rebuild of up to 2,000 rows on every
   collapse/filter/focus toggle (2491-2522 via 2459).
4. Per-arc SVG rebuilt each refresh (drawGutterArcs 2529-2605); assignLanes O(spans x lanes).
5. renderCanvas: cy.destroy() + full re-instantiate + sync layout every render (1407-1427).
6. Cheap toggles (linked-only, preset switch) re-issue both SQL queries instead of filtering
   cached rows (3307-3311, 3357-3373).
7. createFileSystemWatcher("**/*") (extension.ts:204): server declares change:NONE and has
   NO didChangeWatchedFiles handler, so every forwarded event is decoded and dropped. Pure
   client-side waste.

Ergonomics/quality:
- ~500-1,000 lines of dead pre-cytoscape DOM-card renderer (addNode/addPins/addEdge,
  Sugiyama stack). The nodes/edges Maps it populated stay empty, so hover cards, pins,
  kind-filter, and follow-cursor centering are silently broken in CANVAS mode (live in list
  mode via separate branches).
- textDocument/references = exact-string spine match (every span interning the same
  StringId, lsp.rs:454-478). No roles, no symbol resolution, no grouping.
- BUG: handle_references builds Locations as root.join(path) against the primary root only;
  multi-repo hits get wrong absolute paths.
- DATA GAP: scip_occurrence.role collapses Import/Read/Write into "reference"
  (role_label, src/scip_import.rs:322) even though the symbol_roles bitmask is parsed.
- No push to the panel: daemon diag_changed reaches the LSP (republish only); the webview
  never learns a tick happened.
- Good bones to keep: marks/type-seed as facts, _node/_edge layer auto-discovery (column
  names bind to renderer roles), window.dlHost 3-call seam (query/hover/open; panel also
  runs in a plain browser via scripts/dl-bridge.mjs), missing-rel tolerance + empty-state
  hints, saved queries, diff preset.

## Track A: performance remediation

First PR, all S and independent:
- A1 Drop synchronize.fileEvents (or scope to `**/.dl/*.dl`); keep the marks.dl watcher.
- A2 Cheap toggles re-render from cached lastNodeRows/lastEdgeRows; no re-query.
- A3 dl/query grows {limit, offset, count}: page = `SELECT * FROM (<sql>) LIMIT ? OFFSET ?`,
  count mode returns total only. Host passes limit = render caps and reads back total for
  the "showing N/total" pill. Webview cap stays as browser-bridge guard.
- A4 Skip log_query/refresh_query_log on the panel-read path (telemetry writes on the hot
  lock).

Second wave (M):
- A5 Push-refresh v1: in the existing dl/diagChanged arm (lsp.rs:135-154) also send an
  outbound `dl/graphChanged {tick, paths}` notification; extension forwards to the webview
  like the cursor message; panel debounces ~250ms and re-runs IF visible and an
  auto-refresh toggle is on. (v2, L, later: daemon payload grows the moved-rel set; panel
  intersects with the rels its preset reads.)
- A6 List virtualization: window the row divs (ROW_H=22 fixed, exact math), absolute
  positioning in a full-height container. Arcs untouched: the SVG is already full-height,
  index-math positioned, DOM-independent. listRowEls consumers tolerate off-screen misses
  by scroll-to-index.
- A7 Delete the dead renderer (measureNodeBoxes 1461, addNode 1468-1534, addPins 1535-1604,
  addEdge 1605-1647, partitionPins 1362-1379, Sugiyama 1269-1358, canvas buildLegend
  1898-1918 + applyKindFilter 1919-1939, nodes/edges Maps at 617, verify
  rebuildFlowArrows/runFlips 3135-3166). Keep dual-mode functions' list branches.
- A8 Restore canvas interactivity on cytoscape's own API (rides A7): hover card via
  cy.on('mouseover','node'), pins/highlight via class toggles, centerOnNode via
  cy.animate({center}), findNodeAt canvas branch over cy.nodes().
- A9 Route dl/query to the daemon socket when a daemon runs (daemon query_sql is its own
  engine + thread; LSP loop leaves the panel's critical path entirely).
- A10 Perf harness: seeded big-graph .dl fixture (tunable perf_node/perf_edge rel pair,
  5k/20k/50k) + performance.now marks around run()/renderCanvas/renderList/refreshListView,
  echoed via a `type:'perf'` postMessage; server side reuses DL_PROFILE + stmt_ms +
  _query_log. Fixed interaction script: load -> toggle linked -> collapse -> switch view ->
  follow jump; record deltas per fix.

Later (L): cytoscape incremental element diff (layout only on set change), push-refresh v2,
worker-thread read-only SQLite connection (skip unless daemon-off LSP still bottlenecks;
note the LSP db is in-memory when daemon is off and a program is passed, so the reader path
needs a fallback anyway).

## Track B: references/uses lens + follow-the-user

Resolution ladder, every result labeled with its tier:
1. `compiler`: scip_occurrence covering (line,col) -> symbol; defs/refs from role, impls
   from scip_impl, call edges from scip_fn_edge. Only when index.scip is loaded.
2. `resolved`: identifier -> type_entity.name / call_name.name -> syms; roles from
   type_link (impl/uses/param/returns/field), call_site (call), module_import (import).
3. `textual`: today's string_spans, labeled grep-grade.

Transport: new `dl/refs` request returning grouped RefLens {tier, symbol, declarations,
uses-by-role, containing_types, callers, callees; repo on every hit}. Feeds a "dl
References" TreeView (tier -> repo -> role) + QuickPick AND the flow panel.
textDocument/references rewires to the same resolver flattened to Location[] (fixes the
root.join multi-repo bug by mapping repo slug -> workspace folder, same slug scheme as
writeWorkspaceRepoConfig; existing probe is the no-repo fallback).

Engine seam: `Engine::refs_lens(repo_rel_path, byte) -> Result<Option<RefLens>>` beside
span_at/string_spans/hover (engine/mod.rs:2284-2496). RefHit carries repo, path, 0-based
range, role, container. Callers/callees are 1-hop call_edge lookups (no closure).

Nearly-free LSP features, ranked: documentHighlight (S), workspaceSymbol (S: name LIKE over
type_entity + call_def/call_name), documentSymbol (S-M: type_entity WHERE file=? nested via
parent; breadcrumb backbone), callHierarchy (M), typeHierarchy (M: type_link kind='impl' +
scip_impl). semanticTokens/inlayHint/codeLens deferred.

Follow-the-user: new tiny `dl/locate` request (resolve_span + resolver head) returns
{tier, symbol, displayName, role, repo, file, line}. Extension: 180ms debounce + sequence
guard (stale resolves cancelled). Panel: centers by sym EQUALITY (replaces endsWith
suffix-match); when following, re-scopes via parameterized follow_nodes/follow_edges SQL
with the sym bound as ?1 (type_neighbor generalized). NO fact write per cursor move (a
write would tick the daemon at cursor cadence); fact files stay for explicit cmd+alt+t
seeds. Breadcrumb ring of 8 recent syms as chips; "pin here" snapshots into the existing
pinned set and unchecks follow.

SCIP tier extras: widen role_label (scip_import.rs:322) to emit import|read|write from the
already-parsed bitmask (S) so read/write roles exist in the compiler tier.

Stages: B1 dl/refs resolver tiers 2+3 + TreeView + multi-repo fix (S-M) -> B2 free LSP
features (S, S, S-M) -> B3 SCIP tier + role_label widening (M+S) -> B4 follow-the-user (M,
needs A7/A8 for canvas centering) -> B5 call/type hierarchy (M).

## Track C: BOM structure view

Anchor: the list view already IS the assembly hierarchy (repo -> dir -> file -> type ->
member via path trie + parent column). The BOM upgrade is columns, rollups, sort, and
where-used, never a rewrite. Engine constraint honored: unpinned scc/closure reads are
refused, so the repo-wide table uses pure GROUP-BY aggregates + the dag-layers.dl 2-cycle
longest-path tiering; exploded/where-used views are always pinned on one part (microsecond
condensation BFS).

C1 (S) BOM table: new `.dl/bom.dl` deriving
  bom_node(sym, name, kind, file, line, parent, member_count, fan_in, fan_out, weight) +
  bom_edge(src, dst, kind), riding member_node/member_edge from flow-panel.dl; counts are
  GROUP-BY over call_edge/type_link (fan_in = where-used, fan_out = depends-on,
  member_count = qty, weight = call_def end - line, functions only at first). New bomTable
  preset; renderList reads trailing columns r[6..9] into a right-aligned numeric column
  band; sort chips (fan-in desc default = most depended-on parts first) parameterizing the
  existing sibling comparators. Zero-default via negation-guarded rules or SQL coalesce.
C2 (M) Rollup + where-used: applyCollapse's existing forward pass gains a count-accumulator
  (collapsed group shows subtree totals; edges internal to a collapsed group netted out,
  same rollup the arcs already do, so group fan-in = boundary crossings). Clicking a row
  opens a where-used panel (trace-view row DOM reused): callers via call_site + call_name,
  type refs via type_link by kind, field fill/read via member_edge, importers via
  module_edge; all pinned on the clicked sym; breadcrumb from the row's key/parent chain.
C3 (L) Exploded view: NOT cytoscape. Tier-major reordering of the SAME list renderer:
  bom_tier(file, tier) from module_edge 2-cycle condensation longest-path (copy
  examples/dag-layers.dl); synthetic band rows per stratum; gutter arcs become the leader
  lines (already laned + rolled up); arc multiplicity count rendered at midpoint (reuse
  arcPathChevron vertex loop); an SCC with >1 member renders as one "welded subassembly"
  card with a cycle badge, expandable. Empty states: single stratum note, welded-count
  note, existing cap pill.
C4 (sketch only) 3D iso: chosen mapping z = dependency stratum (the exploded axis becomes
  depth; foundations at back, entry points at front). Fixed 30-degree isometric, 2:1
  projection: screenX = x - z*cos30, screenY = (x + z)*sin30 + y, implemented as CSS
  transforms per stratum band over the unchanged DOM (hover/click keep working). No
  three.js, no orbit. Go/no-go after 2D ships: (1) users actually use stratum ordering,
  (2) typical pinned subassembly has 3-8 strata (>~12 self-occludes, metaphor breaks),
  (3) the CSS prototype stays under one frame at the 2000-node cap. Any failure = stay 2D.

Open questions (answer during C1/C2, none blocking):
- weight for types/fields (no span today): ship functions-only, dot elsewhere.
- fan-in blending: one blended where-used number in the table, split by kind in the
  drill-down.
- tier source: module_edge for the repo-wide table; seeded call_edge SCC for per-crate
  exploded view.
- cross-repo BOM root: repo stays top band (REPO_FILE_COL); consider crate_edge tier
  between repo and module later.
- welded-part UX: inline collapse vs where-used panel.

## Sequencing across tracks

Wave 1 (S batch, one PR): A1-A4, A2's toggle fixes, C1.
Wave 2 (M): A5 push-refresh v1, A6 virtualization, A7+A8 dead-code delete + canvas restore,
  B1 refs resolver + TreeView + multi-repo fix.
Wave 3 (M): B2 free LSP features, B3 SCIP tier, C2 rollup + where-used, A9 daemon routing,
  A10 perf harness (can land any time; ideally before wave 2 to measure it).
Wave 4 — LANDED except C4 (audit 2026-07-11: B4 dl/locate a2b7051/5b640ee, B5 hierarchy 23ada98, C3 exploded stratum 1901bf1; C4 sketch only adaf637): B4 follow-the-user (needs A7/A8), B5 hierarchies, C3 exploded view,
  A11+ cytoscape incremental, push v2, C4 3D go/no-go.

Staffing per Chris: implementation by Opus/Sonnet subagents with exact paths/lines in the
briefs; Fable orchestrates and verifies.

## Verification

- Perf: A10 harness numbers before/after each wave (queryMs/renderMs/rows/drawnArcs on the
  5k/20k/50k fixtures + the fixed interaction script); server stmt_ms/_query_log deltas.
- Refs quality: extend tests/it/oracle_rust.rs to references precision/recall vs
  rust-analyzer's SCIP on this repo; printed delta per stage. Multi-repo fix proven with a
  two-fixture-repo collision test (same rel path in both; URIs must differ).
- Follow: scripted cursor walk asserts sym-exact centering and ZERO .dl writes during a
  follow session; stale-resolve cancellation asserted.
- BOM: unit-test the applyCollapse count rollup (pure function, node harness exists);
  bom.dl rails run under dl --check; e2e asserts counts match hand-computed GROUP-BYs on a
  small fixture.
- Extension install: version-bump the vsix (same-version reinstall silently no-ops while
  VS Code runs).

## Critical files

- src/lsp.rs (handle_query 392-405, diagChanged arm 135-154, handle_references 454-478,
  new handle_refs/handle_locate, capabilities 46-63)
- src/engine/mod.rs (query_sql 2515-2546, refs_lens near span_at/hover 2284-2496)
- src/scip_import.rs (role_label 322)
- src/daemon.rs (query_sql 1330, broadcast_diag_changed 435-461)
- editors/vscode-dl/src/extension.ts (watcher 204, row cap 567, dlHost bootstrap 527-547,
  resolveOpenUri 68-80, onDidReceiveMessage 557-585)
- editors/vscode-dl/media/flow-panel.html (render 3046-3066, renderCanvas 1380-1435,
  refreshListView/renderListRows 2459-2522, drawGutterArcs 2529, buildRows 1960,
  applyCollapse 2132, dead renderer 1269-1939 subset, PRESETS 696)
- .dl/flow-panel.dl (member_node/member_edge), new .dl/bom.dl
- examples/dag-layers.dl (tiering to copy), examples/symbol-profile.dl (where-used shape)
- tests/it/oracle_rust.rs (references precision/recall extension)
