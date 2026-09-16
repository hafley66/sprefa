# REPORT2: dock strip + who-called-who (night prototype, DISPOSABLE)

Branch lab/dock-strip, base ce710f1. No commits. Nothing written outside this
worktree. Never ran `just dev`.

## Gates (FIRST / SECOND actions)

- `git merge --ff-only ce710f1` -> "Already up to date" (was already at base), exit 0.
- `corepack pnpm@10.12.4 install --prefer-offline` -> done in 8.4s, exit 0.

## Built

Rust (`src-tauri/src/harness.rs`)
- `HarnessTraceRow` extended with `parent_id: Option<String>` and
  `parent_kind: Option<&'static str>` (serde camelCase -> `parentId`/`parentKind`).
- `trace_claude` now also walks `<projectDir>/<parentSessionId>/subagents/*.jsonl`
  and emits child seeds with `parent_id = <parentSessionId>`,
  `parent_kind = "subagent"`. Reused the existing `isSidechain` evidence
  (`read_claude_transcript`) for the top-level exclusion. Other harnesses stay
  parentless (rust stays ignorant of mail, same split as the duel).
- Proof #2 test `claude_subagent_child_carries_parent_id` proves the child seed
  carries parent_id on a fixture HOME tree.

Frontend (`src/plugins/harnessTrace/`)
- `0_types.ts`: frozen `AgentSessionNode`, `HarnessTraceRow`, seam
  `HarnessTraceSeed` (now with parent fields), `MailEnvelope`, `MailRegistry`.
- `0_tree.ts`: `toAgentNodes` (seam rows -> frozen model keyed by session id),
  `resolveDispatchParents` (attaches `parentKind="dispatch"` when the envelope
  sender resolves through the registry to a live node), `buildAgentTree`
  (flat -> roots/children; orphan child whose parent is absent is promoted to a
  root, never dropped). Pure, vitest-covered.
- `0_tree.test.ts`: proof #1 (roots + children, orphan promotion, dispatch-parent
  resolution, plus the no-overwrite of a rust subagent parent and the seam map).
- `0_mail.ts`: `enrichRows` passes parent fields through the spread.
- `DockStripPanel.tsx`: the strip tree (TreeTable, tree column first), mail
  fs-watch leg, plugin state for sorting.
- `index.ts`: second panel `dock-strip` registered beside `harness-trace`.

Dock (`src/reactdock.tsx`)
- `toggleStripPanel` + `stripState` + `PluginDef.bottomStrip`. Persists
  open/height under plugin id `dock-strip`. New bottom group mounts the strip in
  its own anchored group, sized from the persisted height (default 220).
- `togglePanel` routes `bottomStrip` panels to `toggleStripPanel`.

E2E (bottom strip + screenshot baseline)
- `e2e/dock-strip.tsx`, `e2e-dock-strip.html`, `e2e/dock-strip.spec.ts`.
- Baseline minted with `--update-snapshots`:
  `e2e/dock-strip.spec.ts-snapshots/dock-strip-darwin.png` (30674 bytes).

## Receipts

| Gate | Command | Output | Exit |
| --- | --- | --- | --- |
| merge | `git merge --ff-only ce710f1` | Already up to date | 0 |
| install | `corepack pnpm@10.12.4 install --prefer-offline` | Done in 8.4s | 0 |
| tsc | `corepack pnpm@10.12.4 exec tsc --noEmit` | only `src/plugin.test.ts(69,64)` CtxItem label (known base red) | 2 |
| cargo-check | `just cargo-check` | Finished dev profile | 0 |
| cargo test harness | `cargo test harness:: --manifest-path src-tauri/Cargo.toml` | 5 passed incl. claude_subagent_child_carries_parent_id | 0 |
| vitest | `corepack pnpm@10.12.4 exec vitest run src/plugins/harnessTrace/` | 3 files, 21 tests passed | 0 |
| e2e (mint) | `corepack pnpm@10.12.4 exec playwright test e2e/dock-strip.spec.ts --update-snapshots` | 1 passed | 0 |
| e2e (verify) | `corepack pnpm@10.12.4 exec playwright test e2e/dock-strip.spec.ts` | 1 passed | 0 |

`just build`/`just test` intentionally not run (base-red vega/panelZoom; those
files untouched), per CONTRACT2 Gates.

## Supersession note (recorded per CONTRACT2 Tree law)

CONTRACT2 supersedes the duel CONTRACT.md's "drop claude subagents" filter rule.
Subagent sessions now COME BACK into the data, but only as children beneath
their claude parent, collapsed by default; they are never top-level. The rust
top-level walk still excludes them (via `isSidechain`), and the new subagent
walk re-emits them as child seeds with `parent_id`.

## Deviation: concurrent writer in this worktree (STOP-and-report item)

Mid-lane, files in `src/plugins/harnessTrace/` and `src/worktrees.ts` were
rewritten by a second author active in the same worktree, on top of my
foundation:

- New files I did not author: `src/plugins/harnessTrace/2_join.ts` +
  `2_join.test.ts` (tmux-session join), and an edit to `src/worktrees.ts`
  wiring `setDockStrip({ onOpen })`.
- My files rewritten after I wrote them: `0_types.ts` (added
  `tmuxSession: string | null` to the frozen `AgentSessionNode`) and
  `DockStripPanel.tsx` (tmux column, `attachTmux`, click = open the joined
  session, an inline `<style>`). `index.ts` later edited again (my `onRemove`
  persistence dropped, `keepAlive: true` added). My e2e `dock-strip.spec.ts` /
  `dock-strip.tsx` were replaced by the second author's version.

Per CONTRACT2 "if reality deviates, STOP and record instead of improvising", I
stopped rewriting those files rather than fight the other author. Two notes from
that:

1. Frozen-model deviation (recorded, not silently merged): `AgentSessionNode`
   gained a `tmuxSession` field and a click-to-open-tmux behavior that CONTRACT2
   does not specify. It stays in place only because the other author owns it;
   it is outside the frozen state model.
2. The bottom strip was not rendering visibly through the manual
   `addGroup`+`moveTo` path (panel content stayed in a zero-size
   `dv-render-overlay`). I moved `toggleStripPanel` to the contract-prescribed
   `addPanel({ position: { direction: "below" } })`, which creates and activates
   the bottom group so it lays out. That fix (in `src/reactdock.tsx`, my file)
   is what made the strip mount and the e2e pass.

Everything else in my lane matches CONTRACT2; no other items were stopped.
