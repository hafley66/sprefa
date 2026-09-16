# CONTRACT3: strip moves INSIDE the terminal tab + per-tab router (supersedes CONTRACT2 placement)

User ask, verbatim intent: "i want to see session in a tab panel, then within
that, like the sidebar of files and session turns we have, i want a bottom
panel that shows them and their relation/nesting" and "if you click a subagent
row, it pushes the internal router as well in that tab so we can hit back
button to go back".

The worktree is DIRTY with the CONTRACT2 lane's uncommitted work. That is the
base you build on. Do NOT reset, clean, stash, or revert anything.

## 1. In-tab strip (placement change)

- `TerminalPanel` (src/reactdock.tsx, ~line 222) becomes a flex COLUMN:
  `.term-slot` on top (flex 1), the relation strip underneath it, the existing
  `SessionSidebar` unchanged on its side placement.
- Strip height: autofit to content, capped. `max-height: 240px`,
  `overflow-y: auto`, no fixed height. Zero related rows = strip not rendered
  at all (no empty bar).
- Strip content: the SAME TreeTable/columns as `DockStripPanel.tsx` (reuse its
  COLUMNS and data path via a shared exported component, do not copy-paste the
  column defs), FILTERED: show only trees that contain at least one node whose
  `tmuxSession` equals THIS terminal's tmux session name (`termSid` of the
  panel). Keep the whole containing tree with nesting; never flatten.
- The global bottom-group strip (`toggleStripPanel`, DockStripPanel
  registration) STAYS in place and working. The in-tab strip is additive.
- xterm fit: after the strip mounts/resizes, call `hooks.onTermLayout(sid)`
  (same refit pattern the sidebar toggle uses at reactdock.tsx ~line 254).

## 2. Per-tab internal router (new, small)

- New file `src/plugins/harnessTrace/3_router.ts` + `3_router.test.ts`.
  Declare the interface in `0_types.ts` (project law: every new class/interface
  in the package's header types file, `I` prefix).
- Shape (sync, plain; no rxjs needed here, follow the file-local style of the
  plugin which is React state + store):
  - `ITermRouter`: per-terminal stack of views.
    `push(sid: string, view: TermView)`, `back(sid: string): TermView | null`,
    `current(sid: string): TermView | null`, `canGoBack(sid: string): boolean`,
    plus a subscribe for React.
  - `TermView = { kind: "agent-session"; agentSessionId: string }` (one kind
    today; the type exists so more kinds can join).
  - In-memory Map keyed by terminal sid is fine; no persistence required.
- Click behavior: clicking ANY row in the in-tab strip (subagent or not):
  - if the row has a `tmuxSession` join, KEEP the existing bridge behavior
    (open that tmux session) — unchanged from CONTRACT2;
  - AND push `{kind:"agent-session", agentSessionId: row.id}` onto this
    terminal's router stack.
- Back: while the stack is non-empty, the strip header shows a back button
  (`←`). Clicking it pops. The CURRENT router view is displayed as a header
  line in the strip ("viewing: <agentSessionId>") so the push/pop is visible;
  wiring the sidebar turns view to the router top is OUT OF SCOPE for this
  lane (record it as a follow-up in REPORT3.md).

## 3. Proofs

1. `3_router.test.ts` (vitest): push/push/back/back order, per-sid isolation,
   back on empty stack returns null and canGoBack false.
2. In-tab filter unit test: a forest where tree 1 contains a node joined to
   tmux "s1" and tree 2 does not; filtering for "s1" keeps all of tree 1
   (including its unjoined children) and drops tree 2.
3. New e2e `e2e/dock-strip-in-tab.spec.ts` + host tsx/html, cloned from the
   existing `e2e/dock-strip.spec.ts` camera mechanics
   (`__instantE2eNativeResults`, function-valued fixtures, stub sessions
   panel): mounts a terminal panel host (a stub div standing in for the xterm
   node is fine) with the strip beneath, fixture tree with a claude parent +
   subagent child joined to the host's tmux session. Assertions:
   - strip renders under the term area, height <= 240 with the fixture data;
   - click the subagent row -> "viewing: <id>" header appears (router pushed);
   - click back -> header clears (router popped);
   - screenshot baseline `dock-strip-in-tab-darwin.png` minted with
     `--update-snapshots`, then a verify run passes.

## 4. Gates (run all, record outputs in REPORT3.md)

| gate | command |
| --- | --- |
| install | `corepack pnpm@10.12.4 install --prefer-offline` |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` (known base red: `src/plugin.test.ts(69,...)` CtxItem — acceptable, everything else must be clean) |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/plugins/harnessTrace/` (all pass, incl. the 21 existing) |
| e2e mint | `corepack pnpm@10.12.4 exec playwright test e2e/dock-strip-in-tab.spec.ts --update-snapshots` |
| e2e verify | `corepack pnpm@10.12.4 exec playwright test e2e/dock-strip-in-tab.spec.ts` |
| e2e old | `corepack pnpm@10.12.4 exec playwright test e2e/dock-strip.spec.ts` (must still pass) |

No rust changes expected; do not touch `src-tauri/` (its data already carries
parent/tmux fields).

## 5. Laws

- No commits. Nothing written outside this worktree. Never `just dev`.
- If reality deviates from this contract, STOP that item and record it in
  REPORT3.md instead of improvising. A permission denial ends that approach.
- Comments only for constraints the code cannot show. No em dashes. Never the
  words provenance, substrate, load-bearing, regime. Descriptive names, never
  single letters. Do not edit files outside your scope: your files are
  `src/reactdock.tsx` (TerminalPanel + strip mount only),
  `src/plugins/harnessTrace/*`, `e2e/dock-strip-in-tab.*`, `REPORT3.md`.
- Deliverables: REPORT3.md (gates table + deviations) and
  `e2e/dock-strip-in-tab.spec.ts-snapshots/dock-strip-in-tab-darwin.png`.
