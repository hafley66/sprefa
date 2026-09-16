# CONTRACT2: dock strip + who-called-who (night prototype, DISPOSABLE)

This lab extends the harness-trace work already present in this branch (base
ce710f1 carries the panel under src/plugins/harnessTrace/ plus an e2e camera).
It is a prototype: the duel verdict is pending and NOTHING here merges anywhere.
Owner-set goals, verbatim: "a bottom panel within the tmux view, and a way to
know what harness called who as a subagent etc., with a simple state model."

## State model (frozen — implement exactly this)

```ts
export interface AgentSessionNode {
  id: string;                    // session id
  harness: "claude" | "opencode" | "codex" | "kimi";
  parentId: string | null;       // the session that caused this one, else null
  parentKind: "subagent" | "dispatch" | null;
  // "subagent": claude-in-claude, parent = the session whose dir contains it
  // "dispatch": cross-harness, parent = sender session resolved from the mail
  //             ledger (envelope.from via registry), null when unresolvable
  from: string;                  // dispatcher agent name ("user" when none)
  why: string;                   // envelope body first line ("" when none)
  ts: string; lastActivity: string;
  status: "live" | "idle" | "done" | "dead";
  cwd: string;
  tmuxSession: string | null;   // the instant tmux session this agent runs in
}
```

## Click = go there (owner-set, core behavior)

The strip is, in the owner's words, "really a list of tmux's that we are
related to": clicking a row must focus that session's terminal, exactly like
clicking a subagent in Claude Code's own TUI jumps to it.

- Join (frontend): tmuxSession = the store tmux session (same store rows
  TmuxPanelV2 reads) whose pwd or any chip path equals the node's untildified
  cwd; when several match, prefer the one whose proc names the harness binary
  (claude/opencode/codex/kimi). No match = null.
- Click on a row with tmuxSession != null invokes THE SAME open path
  TmuxPanelV2 uses: extend registerV2Bridges in src/worktrees.ts with a
  setDockStrip bridge exposing onOpen(sessionName) -> the existing openTab
  mechanism (mirror the setTmuxPanel precedent at worktrees.ts:868-884; do not
  import UI internals directly into the plugin).
- Rows with tmuxSession == null render the session cell dimmed and click is a
  no-op (no error, no toast).
- Follow the click idiom TmuxPanelV2 itself uses for opening (read it; if it
  opens on double-click, the strip opens on double-click too).

Tree law: top-level rows are sessions with parentId == null. Children render
UNDER their parent via TreeTable tree expansion (treetable.tsx supports it; see
its expansion API). Claude subagent sessions therefore COME BACK into the data
(the duel filter dropped them) but ONLY as children, never top-level — collapsed
by default. This supersedes the duel CONTRACT.md filter rule; record the
supersession in REPORT2.md.

## Rust change

Extend the existing harness_trace_rows reader (this branch's version):
- claude: also walk <projectDir>/<parentSessionId>/subagents/*.jsonl, emitting
  child seeds with parent_id = <parentSessionId>, parent_kind = "subagent".
  Reuse the existing isSidechain evidence; do not re-derive it.
- other harnesses: no disk parent exists; parent_id stays null (the frontend
  mail join may attach parentKind "dispatch" from envelopes; rust stays
  ignorant of mail, same split as the duel).
- Keep the serde field names camelCase like the branch's existing struct.

## Bottom strip (the "tmux view" ask)

No hand-rolled global chrome. Achieve "bottom panel" with the dockview API the
repo already uses: on activation the strip panel is added in a bottom group
(addPanel with position: { direction: "below" }, reference = the active group)
sized ~220px. Read src/reactdock.tsx (togglePanel, applyLayout, buildDefault)
and follow its idioms; persist open/closed + height via
readPluginState/savePluginState under "dock-strip". The strip content = the
AgentSessionNode tree (TreeTable, columns copied from the branch's panel with a
tree column first). Register it as a second panel in the EXISTING harnessTrace
plugin (panels array supports two entries; see src/plugin.tsx PanelDef).

## Proof (mandatory)

1. Unit: vitest for the tree-building fn (flat nodes -> roots+children), incl.
   an orphan child (parentId pointing at a session not in the list -> promoted
   to top-level, never dropped) and a dispatch-parent resolution from envelopes.
2. Rust: one test on a fixture HOME tree containing a fake
   <project>/<parent>/subagents/agent-x.jsonl proving the child seed carries
   parent_id.
3. Camera: extend the existing e2e page/spec pattern with a NEW spec
   e2e/dock-strip.spec.ts: fixtures = one claude parent with two subagent
   children (one live one done) + one cross-harness dispatch child
   (opencode session with parentKind dispatch via mail fixtures), plus store
   tmux sessions whose pwd values make two of those rows join a tmuxSession.
   Assert: the strip mounts in a BOTTOM group; expanding the parent shows the
   children; activating a joined row calls the bridge onOpen with the right
   session name (spy via the e2e mount, the way the page wires bridges);
   activating an unjoined row calls nothing. Screenshot (--update-snapshots)
   so the morning review has a PNG showing children under parents with the
   tmux join visible.
4. Unit: the join fn (nodes x tmux rows -> tmuxSession) covering pwd match,
   chip-path match, proc tie-break, and no-match null.

## Gates

just check green EXCEPT the known base plugin.test.ts:69 tsc error…
NOTE: this branch (fable lane) did NOT patch that base error, so tsc is red at
base here. Gate = `corepack pnpm@10.12.4 exec tsc --noEmit` output contains ZERO
errors mentioning your files; cargo-check exit 0; your vitest files green;
cargo test harness:: green. just build/test stay base-red (vega, panelZoom):
do not touch those files.

## Laws

Repo AGENTS.md binds (TreeTable only, <500-line files, one panel per file,
never `just dev`). STOP-and-report on any deviation. No commits. No writes
outside this worktree. REPORT2.md at root: built list, receipts, the filter-rule
supersession note, deviations.
