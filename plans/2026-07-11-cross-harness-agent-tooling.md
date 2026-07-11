# Cross-harness agent tooling: .agents/ once, every CLI (Claude Code / Codex / opencode)

## Context

Chris uses Claude Code, Codex CLI, and opencode interchangeably and wants one set of
skills/hooks/commands/subagents/agent_* rels working in all of them. dl is already the
adapter binary: `dl setup` wires harness config, `dl --hook` is the hook body, the
`AgentHarness` trait already reads two transcript stores. The design was anticipated —
hook.rs:21 says "a second harness = a second render-style arm here."

**Decided**: staged delivery (arc 1 = hooks + skills, the daily-use pair; arc 2 =
commands/subagents/codex-reader/MCP-LSP wiring), and `.agents/` is the single
authoring home (the open convention Codex and opencode already read natively; dl's
own shipped skills stay embedded in assets/).

## Research facts that shape the design (2026-07-11 sweep)

- **Codex CLI** (CORRECTED 2026-07-11 against the installed codex 0.144.1): reads
  `.agents/skills` natively (explicitly NOT `.claude/skills`); hooks live in
  **`.codex/hooks.json`** in exactly the Claude Code `settings.json` `hooks` shape
  — NOT in `config.toml`, which only stores codex-managed `[hooks.state]` per-hook
  `trusted_hash` entries. The hook stdin/stdout JSON is the **Claude Code wire
  format verbatim** (verified against the binary's embedded draft-07 schemas:
  input = hook_event_name/session_id/cwd/model/permission_mode/tool_*/
  transcript_path/turn_id; output = BlockDecisionWire + hookSpecificOutput, both
  additionalProperties:false). TRUST WALL: codex runs a hook only after the user
  trusts it in codex's UI (hash formula not externally reproducible; only bypass
  = `--dangerously-bypass-hook-trust`) — live hook fire could not be captured;
  `dl setup` writes hooks.json and prints a loud trust instruction, never forging
  hashes. Consequences: NO toml_edit dependency; the codex dialect is a
  byte-compatible alias of the claude arm; auto-detect never picks codex.
  Subagents are TOML (`.codex/agents/*.toml`); custom prompts
  `~/.codex/prompts/*.md`; MCP via `[mcp_servers.*]`.
- **opencode**: reads `.agents/skills`, `.claude/skills`, AND `.opencode/skills`; NO
  native hook config — lifecycle events only via JS plugins (`.opencode/plugins/*.js`,
  events like `tool.execute.before/after`, `session.*`, `message.*`); subagents
  markdown `.opencode/agents/*.md` (near-Claude format); commands
  `.opencode/commands/*.md`; MCP + LSP config in `opencode.json`.
- **Claude Code**: the only harness needing shims — `.claude/skills/` (symlink),
  `settings.json` hooks (already implemented, setup.rs:481).
- **Rust crates** (researched — nothing does the full render layer; that space is JS
  `rulesync` / Go `agentsync`): toml_edit NOT needed for arc 1 (codex hooks are
  hooks.json JSON, see the corrected codex facts; toml_edit deferred to arc 2's
  md→TOML `.codex/agents`), **jsonc-parser `cst` feature** (format-preserving opencode.jsonc
  edits, only real option), **gray_matter** (frontmatter READ; emit is ~100 LOC over
  serde_yaml). SKIP `skill`/`agentsync`/`claude-hooks` crates — 0.x deps duplicating
  plumbing dl already has.
- dl-side inventory: the Claude coupling is concentrated in exactly four spots —
  settings.json hook registration (setup.rs:481-551), the hook stdin/stdout JSON
  dialect (hook.rs), `.claude/skills` resolution paths (hook.rs:32, setup.rs:384),
  and the ClaudeCodeJsonl transcript reader (agent.rs:59). The rels
  (hook_event/agent_edit/agent_touch/skill_loaded) are already harness-neutral with
  harness as a column value; `dl --mcp`/`dl --lsp` are protocol-standard. `.agents/`
  is currently authoring-convention only — nothing in src/ reads it.

## Arc 1 (build now): hooks dialect seam + skills unification

### A. Skills — `.agents/skills/` as the read path everywhere (S)

- `wire_repo_skills()` (setup.rs:384): additionally scan
  `<repo>/.agents/skills/*/SKILL.md` and symlink each into
  `.claude/skills/<name>/SKILL.md` (relative links, idempotent, same pattern as the
  existing assets/*.skill.md loop, which stays for dl's shipped skills).
  Codex/opencode need nothing — they read `.agents/skills` natively.
- `resolve_skill()` (hook.rs:32) search order becomes: `<root>/.agents/skills` →
  `<root>/.claude/skills` → `~/.agents/skills` → `~/.claude/skills`.
- Global `wire_global()` (setup.rs:203): write the embedded sprefa-dl skill to
  `~/.agents/skills/sprefa-dl/SKILL.md` as the primary copy; keep the `~/.claude`
  copy and the opencode `skills.paths` wiring as-is.

### B. Hooks — parse/render dialect seam in hook.rs (M)

```rust
enum HookDialect { ClaudeCode, Codex, OpenCode }
// parse arm:  (dialect, stdin json) -> HookEvent { kind, session, json }
//             kind/session field mapping per dialect; raw json kept whole
// render arm: (dialect, inject/inject_skill/block rows) -> stdout json per dialect
```

- `dl --hook` gains `--dialect claude|codex|opencode` (default auto-detect from
  payload fields; claude = `hook_event_name` present). The rel contract
  (`inject`/`inject_skill`/`block`) and the daemon-first feed are UNTOUCHED —
  dialects are pure I/O arms.
- **Codex arm** (AMENDED per the corrected facts above): the payload IS the Claude
  wire format, so the codex parse/render arms alias the claude arms (emit nothing
  extra — the output schemas are additionalProperties:false). `wire_codex_hook()`
  in setup.rs writes `<repo>/.codex/hooks.json` via the same JSON-merge path as
  wire_claude_hook (`register_hook_event`, dedup by command substring) — no
  toml_edit. Setup prints the trust instruction (user approves the hooks in
  codex's UI before they fire).
- **opencode arm**: embedded JS plugin (new `assets/dl-opencode-plugin.js`, ~100
  lines) written by `dl setup --project` to `.opencode/plugins/dl.js` (consent-gated,
  like wire_claude_hook). Plugin subscribes `tool.execute.after` (PostToolUse parity)
  + the prompt/message event (UserPromptSubmit parity — confirm exact event name
  during impl), translates each event to the NEUTRAL shape `{kind, session, json}`,
  shells `dl --hook --dialect opencode`, applies the response (inject →
  prompt/context append via plugin API; block → deny in `tool.execute.before`). The
  opencode parse arm accepts the neutral shape directly — our plugin, our schema.
- `hook_event` rel: zero engine change. Existing `.dl` condition programs
  (chat-marks, hook-skill-on-test) work on all three harnesses unchanged.

### C. Setup UX

`dl setup --project` detects/offers all three: `.claude/settings.json` hooks
(existing), `.codex/hooks.json` hooks (new), `.opencode/plugins/dl.js` (new), each
idempotent and consent-gated on TTY like today (setup.rs:355-370).

### Files touched

| file | change |
| --- | --- |
| src/hook.rs | HookDialect enum, parse/render arms, resolve_skill path order |
| src/setup.rs | wire_repo_skills .agents/ scan, wire_codex_hook, wire_opencode_plugin, global .agents copy |
| src/cli/mod.rs | --dialect flag + doc text de-Claude-ing |
| assets/dl-opencode-plugin.js | new embedded plugin |
| Cargo.toml | unchanged for arc 1 (toml_edit / jsonc-parser cst / gray_matter deferred to arc 2) |
| tests | dialect parse/render units; setup idempotency e2e (pattern setup.rs:584-696); hook e2e per dialect, canned payloads |

### Verification

- Unit: parse arm per dialect (canned payloads incl. missing-field tolerance), render
  arm per dialect (inject/skill/block × dialect matrix).
- e2e: `dl --hook --dialect X` per canned payload against a scratch engine; setup runs
  twice → identical tree; a `.agents/skills` skill resolves through `inject_skill`.
- Live smoke: scratch repo, `dl setup --project`, trigger one PostToolUse in each of
  the three CLIs, confirm the inject lands. Codex schema capture happens BEFORE the
  codex arm is written.
- Suites + magic-rel audit green.

## Arc 2 (sequenced next, not built here)

Commands + subagents render from `.agents/` (`.agents/commands/*.md` →
`.claude/commands` / `~/.codex/prompts` / `.opencode/commands`;
`.agents/agents/*.md` → symlink `.claude/agents`, near-copy `.opencode/agents`,
md→TOML `.codex/agents` via toml_edit); a `CodexSessions` AgentHarness arm for
agent_* rels (session-store format needs research); MCP config wiring (.mcp.json /
`[mcp_servers.dl]` / opencode.json `mcp`) + opencode `lsp` entry for `dl --lsp`.
gray_matter + the frontmatter emitter earn their keep here.

## Staffing

Arc 1 = one Sonnet worktree agent (hook.rs + setup.rs are cohesive; suite budget 2
full runs), base SHA named in the brief. The codex hook schema verification is done
first on a live codex install and the captured payload pasted into the brief.
