# Lane: wrapped-line link fix (instant terminal)

Worktree /Users/chrishafley/projects/instant-lab-linkwrap, branch lab/linkwrap,
base 0e4e017. FIRST action: `git merge --ff-only 0e4e017` — failure = STOP and
write REPORT.md saying so. SECOND: `corepack pnpm@10.12.4 install
--prefer-offline` (>5 min or error = STOP and report).

Read CONTRACT.md at the worktree root; it is the whole spec and it binds you.

Code to read before writing:
- src/terminal.ts:480-620 (link provider, wordAt, ⌘-click paths)
- src/termTokens.ts (the one scanner; you do NOT modify it)
- e2e/term-cmd-hover.spec.ts (terminal e2e mechanics to reuse)

Rules: no commits; nothing outside this worktree; never `just dev`; deviations
recorded in REPORT.md, never improvised around; a permission denial ends that
approach. Deliverables: REPORT.md + the tests and (if feasible) e2e named in
CONTRACT.md.

Style: comments only for constraints code cannot show; no em dashes; never the
words provenance, substrate, load-bearing, regime; descriptive names.
