# Programmable hooks via LSP + agent-harness guardrail injection

Two related ideas captured 2026-06-28 (separate from the autodoc / spec / nav
work; not scheduled). Both ride one seam: sprefa's daemon as an always-on policy
engine that the editor LSP and the agent harness can poke, with the policy itself
expressed as dl rules.

## A — programmable hooks via LSP, and harvest any data a hook exposes

Want: user-defined hooks that fire through the LSP, plus capture of every datum a
hook makes available (the request, the file, the cursor, the diagnostic, the
tool-call payload). Today the LSP surface is fixed handlers (definition /
references / hover / diagnostics) reading convention rels (`def_target`, `diag`).
Generalize:
- a hook is a dl rule whose head is a reserved sink (like `diag`/`gen`) keyed by a
  trigger event: `on_save`, `on_hover`, `on_open`, `on_tool_call`, `on_turn`.
- the daemon feeds each event's data in as a source row (so the rule body can join
  it against the code graph), and reads the rule's output sink (a message, an
  edit, a diagnostic, a command) back out.
- "any data we can get from hooks": treat every hook event as a typed source
  relation (`hook_event(kind, json)`), so a page can pattern-match the raw payload
  and lift whatever fields it wants — no per-field engine code.
- Composes with the DSL-driven hover-section idea (cross-lang-goto plan I4) and the
  off-disk pull source (those are just more event/source kinds).

## B — agent-harness guardrail injection ("hey, don't do that")

Motivating case: an assistant emitted a word the user bans from AI-authored files
(same class as the existing CLAUDE.md word bans). Want: a live guardrail that
detects it in the turn stream and pokes the agent harness with a correction, plus
an onboarding injector ("any AI reading files in here needs THIS message in case
it didn't read CLAUDE.md").

Shape:
- The agent conversation becomes a SOURCE: `turn(idx, role, text, tool_call,
  files_touched)` — fed by the harness's hook events, not from disk.
- Policy is dl rules over that stream, with cron-like / pattern triggers:
  - turn-index trigger (every Nth turn, or turn 0 = onboarding),
  - regex on the current/previous turn text (the banned-word detector),
  - previous tool-call name / args (e.g. "after an Edit to a file under docs/"),
  - file-scope trigger (any AI that READS a file under dir X).
- Output sink: `inject(scope, message)` — the text the harness feeds back to the
  agent (a system/context message: the correction, or the missed CLAUDE.md
  preamble). Same convention-rel pattern as `diag`/`def_target`.
- "LSP integration for agent harness N to be poked": the daemon is the shared
  evaluator; harness N's hook calls it per event, passes the event JSON on stdin,
  gets the injection on stdout, and forwards it to the model.

### Claude Code as the concrete harness interface
Claude Code already exposes the hook events this needs (configured in
settings.json): `UserPromptSubmit` and `SessionStart` can INJECT context (hook
stdout is added to the model's context); `PreToolUse` can BLOCK a tool call and
return feedback; `PostToolUse`, `Stop`, `SubagentStop`, `PreCompact`,
`Notification` give the rest of the stream. So the integration is:
- a thin hook script (settings.json) that pipes the event JSON to `dl` (the
  daemon), which evaluates the policy page and returns the injection / block.
- onboarding injector = `SessionStart` / first-`UserPromptSubmit` hook emitting the
  preamble when CLAUDE.md was not in context.
- banned-word guard = `PostToolUse` / `Stop` hook matching the assistant output;
  on hit, surface a correction (and, for file writes, a `PreToolUse` block so the
  banned token never lands on disk).
- (Verify exact event names + the inject/block contract against the current
  Claude Code hooks docs before building; the claude-code-guide agent or `/hooks`
  is the source of truth.)

### Why sprefa and not a plain script
The trigger predicates are JOINS against the code graph, not just regex: "block an
Edit that introduces a banned identifier INTO a Rust type name", "inject the
contract preamble only when the touched file is under a codegen output dir",
"warn when a turn references a symbol that `reaches()` a deprecated API". That is
the dl engine's job; the hook is just the transport.

## Open questions
- Conversation-as-source ingestion: does the harness give enough per-event data
  (full prior turn text, tool args) on stdin, or only a summary? Gates B.
- Where policy pages live and how they bind to "harness N" (per-repo `.dl/`,
  per-user global, or a policy repo in the multi-repo set).
- Block vs warn posture per rule (PreToolUse exit-2 block is heavy; default to
  warn/inject, opt-in block).
- Statefulness: turn-index / "did they read CLAUDE.md" needs stored per-session
  state — reuse the interruptible/storable activation-state model from the
  off-disk-sources plan.
