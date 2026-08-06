# Lane: turns-on-click (instant relations strip)

Worktree /Users/chrishafley/projects/instant-lab-dock, branch lab/dock-strip
(== instant main). FIRST: `git status --short | head -20` to see the tree
(older lane docs present; ignore them, never reset). SECOND:
`corepack pnpm@10.12.4 install --prefer-offline`.

Read CONTRACT4.md at the worktree root; it is the whole spec and binds you.
Recon section 1 gets written into REPORT4.md BEFORE you build.

Read before writing: src/plugins/harnessTrace/ (InTabStrip, 3_router,
DockStripShared, 0_types), src-tauri/src/harness.rs, e2e/dock-strip-in-tab.*,
src/0_sessionSidebarModel.ts (turn model to mirror, not modify).

Rules: no commits; nothing outside this worktree; never `just dev`;
deviations recorded in REPORT4.md, never improvised; a permission denial
ends that approach. Deliverables: REPORT4.md + updated
dock-strip-in-tab-darwin.png showing the turns view.

Style: comments only for constraints code cannot show; no em dashes; never
provenance, substrate, load-bearing, regime; descriptive names.
