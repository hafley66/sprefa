# Built-in agent-turn relation + per-harness plug (ACP-primary)

Promote the latest-turn guardrail (`v5/examples/latest-turn-guardrail.dl`, the
executable spec) into a first-class sprefa built-in: one Rust trait per agent
harness, so a dl page joins `latest_touch(path)` with zero adapter plumbing.

Mirrors `module_edge` / `scip_*`: the *graph relation* is built-in (Rust producer
behind a registry), the *policy* stays in dl.

## Decision: ACP is the primary source; at-rest readers are the fallback

The Agent Client Protocol (Zed + JetBrains) makes file edits/diffs first-class
*protocol* objects, not something to regex out of a transcript. Both harnesses we
care about already speak it:

| harness | ACP | how |
|---|---|---|
| opencode | native | `opencode acp` |
| Claude Code | adapter | `@zed-industries/claude-code-acp` (Apache), wraps Claude Agent SDK |

Tradeoff that drives the two-tier design:

| | ACP | at-rest (JSONL / SQLite) |
|---|---|---|
| diffs | first-class, live `session/update` | scraped post-hoc, edits-as-regex |
| workflow change | agent must run *through* sprefa (client or proxy) | none — reads files the agent already writes |
| standalone-terminal runs | not covered | covered |

So: ACP when sprefa is in the session path (Zed, or a sprefa proxy); at-rest for
plain `claude` / `opencode` in a terminal. Same trait, three impls.

## 1. Type signatures (Rust, new module `src/agent.rs`)

```rust
trait AgentHarness {
    fn name(&self) -> &'static str;                      // "acp" | "claude-code" | "opencode"
    fn sessions_for(&self, repo_root: &Path) -> Vec<AgentSession>;
}
struct AgentSession { id: String, edits: Vec<TurnEdit> } // newest session for repo_root
struct TurnEdit { idx: i64, role: Role, path: PathBuf }  // path REPO-RELATIVE (strip here)
enum Role { User, Assistant }

fn agent_harnesses() -> Vec<Box<dyn AgentHarness>>;      // [Acp, ClaudeCodeJsonl, OpenCodeDb]

// Producer, called in tick() next to `changed` refresh (engine.rs:4219).
// Gated: runs only if the program references a TURN_REL.
impl Engine { fn refresh_agent_rels(&self) -> Result<()>; }
```

## 2. Built-in relations (register in builtin_rel_decls, engine.rs:206)

| relation | cols | meaning |
|---|---|---|
| `agent_edit` | `(harness:text, session:text, idx:int, path:file)` | every edit in the newest session, repo-relative |
| `latest_touch` | `(path:file)` | edits at `max(idx)` per session, unioned across harnesses |

`latest_touch` computed in Rust (max-idx filter) so the dl page is just
`diag(p,...) <- changed(p), latest_touch(p).` Reserve both names (TURN_RELS),
same guard as SPINE_RELS / scip.

## 3. Adapters (the only harness-aware code)

- **Acp (primary)**: sprefa is an ACP client (spawns the agent) or a passive
  proxy via the `conductor` crate sitting between editor and agent. Tap
  `acpx::subscribe_session_updates()`; `session/update` tool-call + diff messages
  → `TurnEdit` (paths already structured, just relativize). `idx` = update seq.
- **ClaudeCodeJsonl (fallback)**: slug = `repo_root` with non-alnum → `-`; read
  `~/.claude/projects/<slug>/*.jsonl`, newest mtime; line = record, `idx` = line;
  assistant `tool_use` Edit/Write/MultiEdit `input.file_path`. Verified 2026-06-28.
- **OpenCodeDb (fallback)**: `~/.local/share/opencode/opencode.db`;
  `session WHERE directory=:repo ORDER BY time_updated DESC LIMIT 1`; edits =
  `part` rows `json_extract(data,'$.tool') IN ('edit','write')`, `idx` = `seq`,
  path = `$.state.input.filePath`. Verified 2026-06-28.

Breadth reference (NOT a dependency): `cass`
(github.com/Dicklesworthstone/coding_agent_session_search) has Rust connectors
for 20+ harnesses (Codex, Cline, Gemini, Cursor, Aider, Crush, …) → normalized
SQLite. CLI-only, no library crate, and edits aren't first-class there — but its
connector path/format map is the catalog if we want the long tail later. Since
sprefa is SQLite-welded, ingesting its DB directly is an option.

## 4. Lifetimes / storage / refresh sequence

- No new persistent tables: harness artifacts (jsonl / db / ACP stream) are the
  source of truth, read fresh each tick. Only `rel_agent_edit` /
  `rel_latest_touch` materialize (batched `refresh_rel`, never N+1).
- **Reactivity without watching ~/**: the agent's own edit to a repo file
  triggers the tick; by then the artifact already records that edit, so
  re-reading on that tick is in sync. (ACP is even better — the `session/update`
  itself is the wake.) Gate on use so non-agent runs pay nothing.
- Cross-root handled entirely in Rust (strip + session-by-directory); the dl side
  only sees repo-relative `file` paths that join `changed()`.

## 5. Per-harness plug ("play ball in each env")

The daemon is the one evaluator; each env wires its native point to it. New thin
CLI mode `dl --agent-check` ticks and prints latest-turn `diag` rows as JSON, so
every transport is a one-liner.

| env | native hook | wiring |
|---|---|---|
| editor (any) | LSP `publishDiagnostics` | already served under `--lsp`; latest-turn squiggles free |
| Zed / ACP editor | ACP `session/update` | sprefa as ACP proxy (`conductor`) taps diffs live; diag back as an ACP message |
| Claude Code (terminal) | `settings.json` hooks (`PostToolUse`/`Stop`) | hook runs `dl --agent-check`; stdout fed back as context |
| opencode (terminal) | plugin API (`~/.config/opencode`) | plugin calls `dl --agent-check`, surfaces the message |

## 6. Build order
1. `src/agent.rs` trait + the two at-rest adapters + `agent_harnesses()` (unit
   tests vs a jsonl fixture and a temp sqlite). Ship first — no new deps, covers
   terminal runs today.
2. Register `agent_edit`/`latest_touch`, gate, `refresh_agent_rels` in tick. e2e
   test = the `latest-turn-guardrail` fixture, now with zero adapter dl.
3. `dl --agent-check` CLI mode (ticks, emits diag JSON).
4. Acp adapter over `agent-client-protocol` + `acpx` (proxy via `conductor`).
   This is the new-dependency tier; lands after the at-rest tier proves the
   relation + policy.
5. Per-harness transports: CC settings.json snippet + opencode plugin stub +
   ACP proxy entry, each calling `--agent-check` / emitting the ACP message.
   Ship under `examples/integrations/`.

## Open
- Multi-checkout/worktree: CC slug is per-directory, opencode keys on
  `session.directory`, ACP sessions are per-client-cwd — so a worktree gets its
  own session by construction. Correct for free (unlike the ref content-address
  collapse).
- `--agent-check` (stateless, any hook calls it) vs a persistent daemon socket
  (lower per-call latency). Start with the CLI.
- ACP client vs proxy: proxy (`conductor`) is preferred — observes without owning
  the editor UI, so it composes with Zed/JetBrains instead of replacing them.

## Sources
- ACP: zed.dev/acp · agentclientprotocol.com · ACP Rust SDK
  (agentclientprotocol.github.io/rust-sdk) · crates: agent-client-protocol, acpx
- Claude Code via ACP: zed.dev/blog/claude-code-via-acp ·
  npm @zed-industries/claude-code-acp
- opencode ACP: opencode.ai/docs/acp
- breadth catalog: github.com/Dicklesworthstone/coding_agent_session_search
