---
name: project-vscode-ext-review
description: "REVIEWED + PLAN APPROVED 2026-07-10: full eval of editors/vscode-dl done, 3-track design (perf remediation / references lens / BOM view) at plans/2026-07-10-vscode-ext-review.md; implementation NOT started (Chris: planning only); staff impl with Opus/Sonnet subagents"
metadata:
  node_type: memory
  type: project
  originSessionId: ab4a16de-586c-4508-93c0-71986c34bbfa
---

Review COMPLETE 2026-07-10 (Fable orchestrating, 2 Explore + 3 Opus Plan agents). Approved
plan lives at **plans/2026-07-10-vscode-ext-review.md** (copy of
~/.claude/plans/splendid-humming-heron.md). Chris said "planning mode only, dont make
things" so NOTHING is implemented yet.

Key verified findings (evidence in the plan):
- LSP loop single-threaded: dl/query + hover/def/refs + didSave tick serialize on one
  Engine (lsp.rs:156-206); every dl/query does 2 telemetry writes first (lsp.rs:399).
- Triple row-cap waste: engine all rows -> host 20k (extension.ts:567) -> webview 2k
  (flow-panel.html:3046).
- ~500-1,000 dead lines: pre-cytoscape DOM-card renderer; hover cards/pins/follow-cursor
  silently broken in CANVAS mode (list mode works).
- createFileSystemWatcher("**/*") events are DROPPED server-side (no didChangeWatchedFiles
  handler, change:NONE): pure client waste.
- BUG: handle_references root.join(primary root) mislocates multi-repo hits.
- DATA GAP: scip_occurrence.role collapses Import/Read/Write ("reference" only);
  role_label (scip_import.rs:322) discards the parsed bitmask. Widening = S.
- No push to panel (daemon diag_changed reaches LSP for diags only).

Design tracks (waves in the plan): A perf (S batch first: drop watcher, cached-row
toggles, dl/query {limit,offset,count}, skip telemetry writes; then push-refresh
dl/graphChanged, list virtualization ROW_H=22 exact windowing with arcs untouched,
dead-code delete + canvas restore on cytoscape events, daemon query routing).
B navigation (dl/refs grouped RefLens with tier ladder compiler/resolved/textual,
TreeView, textDocument/references rewired + multi-repo fix; free wins documentHighlight/
workspaceSymbol/documentSymbol; dl/locate follow-the-user, sym-equality centering,
parameterized follow SQL NOT fact writes; oracle_rust.rs extended to refs precision/recall).
C BOM (list view IS the assembly hierarchy; bom.dl bom_node with member_count/fan_in/
fan_out/weight GROUP-BYs + sort by fan-in; applyCollapse count rollup; where-used panel
pinned SQL; exploded view = tier-major reorder of the SAME gutter-arc renderer via
dag-layers.dl 2-cycle tiering, cycles = "welded subassembly" cards; 3D iso sketch = z from
dependency stratum, fixed 30deg CSS transform, go/no-go criteria recorded).

Constraint honored everywhere: unpinned closure/scc reads are refused by the engine;
repo-wide views aggregate, drill-downs pin.

PORTABILITY REQUIREMENT (Chris 2026-07-10): the flow panel must stay host-agnostic —
window.dlHost {query,hover,open} + window 'message' events are the ONLY host coupling
(browser bridge scripts/dl-bridge.mjs is the second host). Chris wants to reuse the
panel inside ~/projects/instant as a sprefa-panel plugin later. Never let VS Code API
leak into flow-panel.html; new host->panel signals (graphChanged etc.) go through the
message contract.

Related: [[project-type-shapes-prototype]] (shipped rels the lens uses),
[[reference_vscode_samever_install_trap]] (bump vsix version on install),
[[feedback-sonnet5-for-coding]] (staffing rule).
