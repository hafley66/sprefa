# Lane: in-tab dock strip + per-tab router (instant)

Worktree /Users/chrishafley/projects/instant-lab-dock, branch lab/dock-strip.
The tree is DIRTY with the previous lane's uncommitted work; that is your base.
FIRST action: `git status --short | head -30` to see it. Do NOT reset, clean,
stash, or revert. SECOND: `corepack pnpm@10.12.4 install --prefer-offline`
(>5 min or error = STOP and report in REPORT3.md).

Read CONTRACT3.md at the worktree root; it is the whole spec and it binds you.
CONTRACT2.md and REPORT2.md describe the previous lane (global bottom strip,
tmux join, click bridge) — read them for context; where they conflict with
CONTRACT3, CONTRACT3 wins.

Code to read before writing (all existing, extend rather than rebuild):
- src/reactdock.tsx TerminalPanel (~:222) + the sidebar refit pattern (~:254)
- src/plugins/harnessTrace/DockStripPanel.tsx (COLUMNS, attachTmux, bridge)
- src/plugins/harnessTrace/0_tree.ts + 2_join.ts (tree build, tmux join)
- e2e/dock-strip.spec.ts + e2e/dock-strip.tsx (camera mechanics to clone)

Rules: no commits; nothing outside this worktree; never `just dev`; deviations
are recorded in REPORT3.md, never improvised around; a permission denial ends
that approach. Deliverables: REPORT3.md + the dock-strip-in-tab screenshot
baseline (mint with --update-snapshots, then a clean verify run).

Style: comments only for constraints code cannot show; no em dashes; never the
words provenance, substrate, load-bearing, regime; descriptive names.
