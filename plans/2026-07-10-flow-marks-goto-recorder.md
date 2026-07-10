# Flow marks + goto recorder: named dataflows from editor navigation

## Context

Two asks that compose into one loop:

1. Record every goto jump in VS Code, per named session ("record 1, record
   another"), then union / anti-unify the recordings to reverse-derive a
   dataflow — a flow recorder in the playwright-test-recorder shape.
2. Markdown hover on any span that belongs to a dataflow, a map (the flow
   panel) that centers where the cursor is, and a table of the named flows.

The loop: record or hand-mark a flow -> a named `flow_member` set -> hover
membership on every member span -> panel centers via the existing
follow-cursor seam -> a flows table lists the names.

## Existing assets (verified)

- **Jump tap**: `onDidChangeTextEditorSelection` (extension.ts:544) fires on
  every cursor move incl. cross-file jumps; `e.kind ===
  TextEditorSelectionChangeKind.Command` is the goto-def signature (mouse
  drift is Mouse/Keyboard). Currently panel-scoped; the recorder hoists a
  second, independent subscription.
- **Sessionized ingest**: `hook_event(kind, session, seq, json)` — exact
  shape needed. Reserved builtin, written out-of-tick (engine/mod.rs:362),
  daemon RPC arm at daemon.rs:1445, `insert_hook_event` on the engine.
  chat-marks (`@@mark`) proves the session/seq/argmax .dl idiom.
- **Hover is already markdown**: handle_hover returns
  `MarkupKind::Markdown` (lsp.rs:418). Today it synthesizes from the
  identifier's NAME (entity/callable match); nothing position-anchored and
  nothing user-injectable.
- **Map position**: panel "follow cursor" toggle (flow-panel.html:520,
  1583) + the extension's `{type:'cursor', file, line, word}` post;
  `findNodeAt` (panel:1608) suffix-matches file + line and centers. Works
  for any layer whose nodes carry file/line.
- **Layer discovery**: any `X_node`/`X_edge` rel pair appears in the panel
  as a toggleable layer with zero panel code (the flow-panel-layers
  contract).
- **Sym lift**: `type_entity`/`call_def`/`member_node` spans; the anchor
  resolver (src/anchor.rs) already classifies path:line anchors.
- **Flow graph**: std/flow.dl `flow_edge` union; closure rels cannot be
  read unpinned in a rule body — seed a recursive rule instead (the
  flow-ctor precedent).

## Gap analysis

| need | missing piece |
|---|---|
| record a jump | extension: record toggle + jump classification + transport |
| land it in a rel | LSP request `dl/hookEvent` (one match arm -> `insert_hook_event`); the daemon RPC already exists, this is the in-process twin, and under the thin-client plan the arm forwards to the daemon RPC unchanged |
| user markdown on hover | `hover_note(path, line, col, end_line, end_col, md)` builtin SINK (diag-shaped: reserved, fixed schema, head-written by rules, never `rel`-declared) read at the hover seam |
| named flows | `examples/goto-flows.dl`: sessions -> jump edges -> sym lift -> union/anti-unify -> `flowmark_node`/`flowmark_edge` |
| flows table | free: the `_node`/`_edge` pair IS the table+map layer; hover_note rows derive from membership |

## Type signatures

```rust
// src/lsp.rs — new custom request "dl/hookEvent"
// params {kind: String, session: String, seq: i64, json: String} -> {"ok": true}
fn handle_hook_event(eng: &mut Engine, req: &Request) -> Response;
//   parse params; eng.insert_hook_event(kind, session, seq, json)?;
//   tick (the same quiet tick didSave uses) so goto-flows derives live;
//   under the future LspPump::Daemon arm this forwards to the existing
//   daemon "hook_event" RPC verbatim.

// src/engine/mod.rs — hover_note lookup at the hover seam
// rel_hover_note(path, line, col, end_line, end_col, md); positions 0-based,
// same convention as diag.
pub fn hover_notes_at(&self, path: &str, line: u32, character: u32)
    -> Result<Vec<String>>;
//   SELECT md FROM rel_hover_note WHERE path = ?
//     AND (line < ?1 OR (line = ?1 AND col <= ?2))
//     AND (end_line > ?1 OR (end_line = ?1 AND end_col >= ?2))
//   ORDER BY md; try_rows tolerance (empty when the rel never derived).
// handle_hover appends each md as its own markdown section under a rule
// (---) after the synthesized entity hover; hover_note alone (no entity
// match) still produces a hover.
```

```ts
// editors/vscode-dl/src/extension.ts
interface FlowRecording { name: string; seq: number;
  last?: { file: string; line: number } }
let recording: FlowRecording | undefined;   // per-window, one at a time

// command "dl.recordFlow": prompts start (input box for the name) / stop.
// A status-bar item shows "REC <name>" while active.
// Second selection subscription, ALWAYS on (cheap early-return when not
// recording, panel not required):
//   jump := kind === Command || file !== recording.last?.file
//   on jump: client.sendRequest("dl/hookEvent", { kind: "goto",
//     session: recording.name, seq: Date.now(),
//     json: JSON.stringify({ from: recording.last ?? null,
//       to: { file, line }, word }) });
//   recording.last = { file, line }
// seq = Date.now() ms: monotonic across window restarts, so re-recording
// under the same session name APPENDS (a fresh name per take is the
// expected workflow; the .dl can split takes on seq gaps if ever needed).
```

```
# examples/goto-flows.dl (sketch; final rules are the implementer's)
scan("src/**/*.rs").          # or rider on the user's existing program

# 1. jumps out of hook_event (kind "goto"), json fields via jsonp
goto_jump(session, seq, from_file, from_line, to_file, to_line) <- ...

# 2. sym lift: span containment against call_def/type_entity lines
jump_sym(session, seq, sym) <- ...

# 3. per-session ordered edges (argmax over seq for "previous jump")
flow_take_edge(session, src_sym, dst_sym) <- ...

# 4. union + anti-unify across the takes of one flow name
#    flow_name(session, name) — a fact rel mapping takes to a flow, OR
#    name = session when 1 take = 1 flow
flow_union(name, src, dst)  <- every take's edges
flow_common(name, src, dst) <- edges present in ALL takes of `name`
                               (count(distinct session) == take count)

# 5. panel layer + hover
flowmark_node(sym, name_label, kind, file, line) <- ...
flowmark_edge(src, dst, flow_name) <- ...
hover_note(path, line, col, end_line, end_col,
  "**in flow:** {name} ({role})") <- flow membership x call_def span.
```

## Instance lifetimes

- `recording` (extension): one per VS Code window, dies on stop/window
  close. Recordings themselves persist — hook_event rows live in the db.
- `hook_event` rows: append-only, out-of-tick, survive restarts. GC is the
  user's business (a `DELETE FROM rel_hook_event WHERE session = ?` via
  query_sql, or a retention .dl later; not this arc).
- `rel_hover_note`: derived per tick like any rel; empty unless a program
  heads it.
- Status-bar item: created on first start, hidden on stop, disposed with
  the extension context.

## Storage

No new tables beyond `rel_hover_note` (standard derived rel storage,
full-row set semantics; several notes may target one span — the hover
shows all, sorted). hook_event storage unchanged.

Writes: extension -> dl/hookEvent -> insert_hook_event -> quiet tick ->
goto-flows.dl derives flowmark_*/hover_note. Reads: hover request ->
entity hover + hover_notes_at merge; panel -> layer discovery finds
flowmark; cursor post -> findNodeAt centers.

Uniqueness: hook_event dedups on (kind, session, seq, json); Date.now()
seq makes same-ms double-jumps collapse only if json is also identical
(same from/to — a true dup). flow_common's take count must be
`count(distinct session)` over the flow's takes, not row count.

## Stages

- **F1 (S) hover_note builtin sink**: reserved fixed-schema rel (the diag
  pattern: DIAG_RELS-style guard, catalog group, head-written only) +
  `hover_notes_at` + handle_hover merge. e2e: a .dl heads hover_note, LSP
  hover at the span returns the md; hover off-span doesn't. Use the
  builtin-rel-implementer agent.
- **F2 (S) dl/hookEvent LSP request + recorder**: the match arm in lsp.rs;
  extension command dl.recordFlow + status bar + the always-on
  subscription; package.json command/keybinding. e2e: lsp harness sends
  dl/hookEvent, hook_event row lands, tick ran.
- **F3 (M) goto-flows.dl**: the sketch above made real. Hardest parts:
  the argmax "previous jump" pairing (chat-marks precedent) and the
  anti-unify take count. Unit-style e2e: seed hook_event rows via the RPC,
  assert flow_union/flow_common/flowmark rows.
- **F4 (S) panel + docs**: nothing to build for the map/table (layer
  discovery + follow cursor already do it); verify manually, document the
  workflow in the example header + book if it earns it. CHANGELOG + ledger.

Order: F1 and F2 are independent (parallel agents); F3 needs F2; F4 last.

## Risks / decided trade-offs

- Selection kind is a heuristic: Command fires for goto-def, goto-ref,
  breadcrumb jumps, but also for some non-nav commands. Cross-file change
  catches the rest. Over-capture is fine — anti-unify exists precisely to
  wash noise out across takes.
- Jumps into files outside every workspace folder (stdlib, deps) record
  with a repo-relative miss; keep them as raw paths, the sym lift simply
  finds nothing and the edge stays file-level.
- hover_note is position-anchored, so a stale db shows notes at stale
  spans until the next tick — same staleness class as diag, accepted.
- One recording at a time per window; no cross-window merge (sessions
  union in the db anyway).

## Verification

- F1/F2 e2e in tests/it/ (lsp harness pattern from lsp_symbols.rs).
- F3 e2e seeding hook_event through the real RPC.
- Manual loop: record two takes of the same flow under two names, map
  them to one flow_name, `flow_common` shows the shared spine, hover a
  member fn shows "in flow: ...", follow-cursor centers it, the flowmark
  layer lists both.

## Critical files

- editors/vscode-dl/src/extension.ts (selection sub 544, command wiring)
- src/lsp.rs (handle_hover 400, custom-request dispatch, hook arm)
- src/engine/mod.rs (HOOK_RELS 370, insert_hook_event, diag rel pattern
  for hover_note, hover synthesis)
- src/daemon.rs (hook_event RPC 1445 — the shape the LSP arm mirrors)
- examples/chat-marks.dl (session/seq argmax idiom), std/flow.dl,
  examples/endpoint-flows.dl (diag-on-flow precedent)
- .claude/skills/sprefa-flow-panel-layers (layer contract the flowmark
  pair rides)
