# CONTRACT4: click a strip row -> read that session's turns (router-wired)

User ask: "i would really like to inspect other flash sub sessions." Clicking
a row in the relations strip currently pushes the per-tab router and shows
"viewing: <id>" and nothing else. This lane makes the pushed view show the
selected session's TURNS, for claude and opencode sessions.

Base: current lab/dock-strip (== instant main). The worktree may contain
CONTRACT/REPORT/brief files from earlier lanes; ignore them, no resets.

## 1. Turn source recon FIRST (recorded in REPORT4.md before building)

- claude: transcript jsonl under ~/.claude/projects/... (the harness.rs
  readers know the layout; the session sidebar's turn model reads claude
  ledger turns via src/0_sessionSidebarModel.ts — read it).
- opencode: ~/.local/share/opencode/opencode.db — tables message, part,
  session_message (verified present). Map the minimal columns needed:
  (session id -> ordered turns: role, text preview, ts). Record the actual
  schema you find with receipts.
- codex/kimi: recon only; report the shape, do NOT build readers this lane.

## 2. Build

- Rust (src-tauri/src/harness.rs or a sibling module wired the same way):
  one command `agent_session_turns(harness, session_id) -> Vec<TurnRow>`
  where `TurnRow { role, preview, ts }` (serde camelCase). Two legs: claude
  jsonl, opencode sqlite. Read-only, no schema writes. Unknown harness =
  empty vec, never an error.
- Frontend: when a terminal's router top is `{kind:"agent-session"}`, the
  in-tab strip's body swaps from the relations table to a TURNS LIST for
  that session (same 240px cap, scrollable, newest last), with the existing
  back button returning to the relations table. Reuse the turn-row look from
  the session sidebar where practical; do not modify sessionSidebar itself.
- Types: TurnRow + the seam declared in src/plugins/harnessTrace/0_types.ts.
- The dock-strip global panel keeps its current behavior (relations only).

## 3. Proofs

1. Rust test: fixture claude jsonl -> ordered TurnRows (role+preview+ts).
2. Rust test: fixture opencode sqlite (build a tiny db in the test) ->
   ordered TurnRows.
3. vitest: router top set -> strip renders turns list; back -> relations
   table returns.
4. e2e: extend e2e/dock-strip-in-tab.spec.ts (keep the frozen clock): click
   the subagent row -> turns visible (mock `agent_session_turns` via
   __instantE2eNativeResults); back returns; re-mint the PNG with
   --update-snapshots then clean verify.

## 4. Gates (all, recorded in REPORT4.md)

| gate | command |
| --- | --- |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` (only the known plugin.test.ts:69 base red) |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/plugins/harnessTrace/` |
| cargo | `cargo test --manifest-path src-tauri/Cargo.toml harness` (or the new module's filter; all pass) |
| e2e | dock-strip-in-tab mint+verify, dock-strip verify |

## 5. Laws

No commits. Nothing outside this worktree. Never `just dev`. Deviations:
STOP the item, record in REPORT4.md. Comments only for constraints code
cannot show. No em dashes. Never provenance, substrate, load-bearing,
regime. Descriptive names. Files you own: src-tauri/src/ (the one command +
tests), src/plugins/harnessTrace/*, e2e/dock-strip-in-tab.*, REPORT4.md.
Deliverables: REPORT4.md + updated dock-strip-in-tab PNG showing turns.
