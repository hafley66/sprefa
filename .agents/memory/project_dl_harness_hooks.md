---
name: project_dl_harness_hooks
description: "dl --hook mode (programmable Claude Code harness hooks), landed on main 2026-06-30"
metadata: 
  node_type: memory
  type: project
  originSessionId: 3bd6e49b-8714-4f2d-a9be-7a088c4b0663
---

`dl --hook` = third emit mode beside `--check` (rails) and `--lsp`. dl IS the
Claude Code PostToolUse hook command: reads the event JSON on stdin, ticks, emits
hook JSON (additionalContext / decision:block) on stdout. The CONDITION is a dl
rule — program heads `inject(text)` / `inject_skill(name)` / `block(reason)` over
the agent built-ins. No editor, no bash; settings.json command is just
`dl --hook`. Landed main 2026-06-30 (ae03783 + bench fix 3df44a8, pushed).

Key design rulings (the rough-vs-right cut the user forced):
- Can't call the Skill tool from a hook; injecting the SKILL.md BODY as
  additionalContext is the only mechanism AND stronger than forcing a call.
- LSP is editor-bound (CC has no native LSP client; rust-analyzer reaches CC only
  via an open IDE). Hooks are the only editor-independent, tool-use-triggered,
  context-injecting channel. MCP is pull-only. So force-skill MUST go through a hook.
- DAEMON-FIRST: the daemon (primary mode) already re-ticked on the agent's edit,
  so `--hook` reads the computed emit rels via the existing `query_sql` RPC (no
  new daemon method) instead of re-ticking. In-process cold tick is the no-daemon
  fallback. db::open(None)=in-memory, so default `--hook` can't fight the daemon's
  cache.db lock.
- LOAD-ONCE is DECLARATIVE, not state files: new built-in `skill_loaded(harness,
  session, name)` derived from the transcript (explicit Skill tool_use + dl's own
  prior injection marker "auto-loaded by dl --hook"), refreshed in AgentKind
  alongside agent_edit/agent_touch. Rule negates it. The sidecar-marker-file
  version was the rejected "backwards" shortcut.

Files: src/hook.rs (whole feature), agent.rs cc_skill_loads + AgentHarness::
skill_loads, rels/analysis.rs AgentKind (skill_loaded), main.rs --hook flag,
examples/hook-skill-on-test.dl, docs/skill-injection.md, tests/it/hook_inject.rs
(4 e2e). Built on the same transcript reader as [[project_dl_self_validation_docs]]
(agent_touch is git-free, keyed on --root).

NOT YET: live event not threaded into a `hook_event` rel (condition reads
agent_touch, which is the latest turn = fires correctly but keys on the turn's
edit set, not the exact file of THIS event). Plugin packaging (.claude-plugin/
plugin.json with the hook → turnkey /plugin install) scoped but not built — dl
setup.rs is the natural home. opencode/pi/hermes = one render arm each in hook.rs.
