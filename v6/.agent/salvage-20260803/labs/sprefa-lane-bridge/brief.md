# Lane: agent-bridging design + internet prior-art research

You are a research lane. Worktree: /Users/chrishafley/projects/sprefa-lane-bridge,
branch lane/bridge-research, base 2eceb836. FIRST action:
`git merge --ff-only 2eceb836` — if it fails, STOP and write REPORT.md saying so.

Deliverable: REPORT.md at the worktree root. Nothing else. Do NOT commit. Do NOT
write to any path outside this worktree. If reality deviates from this brief,
STOP and report the deviation in REPORT.md; do not improvise.

## The problem

One human runs four coding-agent harnesses on one macOS box: Claude Code,
Codex CLI, opencode, kimi CLI. We want them to message each other: a
coordinator agent in one harness sends work or questions to a worker agent in
another harness, gets replies, without a human copy-pasting. Call this the
bridge/bus.

## Ground truth already established (read these files FIRST, cite by line)

1. /Users/chrishafley/projects/sprefa/.agent/salvage-20260803/haiku-comms-research.md
   — three prior research reports + a synthesis diagram. Key established facts:
   - `cass` (installed, /opt/homebrew/bin/cass) already indexes all four
     harnesses' transcripts; has `search/pack --robot --json`, `index --watch`,
     and a `swarm` surface that references "Beads + Agent Mail" as an external
     orchestration layer.
   - Injection into a running/resumable session works per-harness today:
     `opencode run -s <sessionID> "msg"` and `kimi -S <uuid> -p "msg"`.
   - Claude Code can wake on file changes via its Monitor tool armed on
     `tail -f` of a mailbox file (<50ms/line).
   - Claude Code has NO native non-Claude subagents (model field is Claude-only);
     three candidate workarounds were listed: babysitter agent shelling opencode,
     an MCP wrapper, a litellm gateway.
2. /Users/chrishafley/projects/sprefa/chat_log/20260802.2.cst-rulings-duels-schemagen-alloy.md
   lines 15 and 21-22 — a ruled NDJSON job protocol precedent (id-tagged jobs,
   `{id,done}` terminator, cancel line, JSON-RPC/LSP framing named as the
   upgrade path) and the bus summary.

## Seed ideas to develop (expand, attack, or replace — say which and why)

- S1 mailbox: `.agent/mail/<agent>.ndjson` append-only per recipient; envelope
  `{id, from, to, ts, kind: request|result|note, reply_to, body}`; one JSON
  object per line, mirroring the extract --serve protocol precedent above.
- S2 wake fan-out: each harness gets the cheapest wake primitive it has —
  Claude Code = Monitor on tail -f; opencode/kimi = a tiny delivery loop that
  watches the mail dir and calls their session-inject commands.
- S3 delivery daemon: one ~30-line watcher (fswatch or tail -F) that routes
  new lines to the right inject command; at-least-once, dedup by id.
- S4 cass as the shared read plane: workers cite `cass pack` output for
  handoffs instead of pasting context; the bus carries pointers, not payloads.
- S5 registry: `.agent/mail/registry.json` mapping agent name -> harness,
  session id, inject command, liveness.
- Open questions to answer with evidence: ack/at-least-once vs fire-and-forget;
  blocking request/reply correlation; who spawns a worker that is not running;
  do mail files belong in the repo or in ~/.agent (multi-repo); does cass index
  the mailboxes automatically (check `cass robot-docs` / `cass capabilities`
  locally — you may run read-only cass commands).

## Internet prior-art sweep (mandatory, keyword-driven)

Use webfetch. For search, fetch these URL shapes (they return HTML/JSON without
auth) and mine the results; 2 failures on a URL = note it and move on:
- https://hn.algolia.com/api/v1/search?query=<kw>  (JSON)
- https://api.github.com/search/repositories?q=<kw>&per_page=10  (JSON, may rate-limit)
- https://duckduckgo.com/html/?q=<kw>  (HTML)

Keywords (run at least these; add your own variants):
"agent mail" MCP; beads issue tracker agents; steve yegge gastown multi-agent;
agent to agent protocol A2A; agent client protocol ACP zed; claude code
subagent other model; multi-agent orchestration tmux; claude-squad; vibe-kanban;
container-use dagger agents; LLM agent message queue filesystem; maildir agent
communication; MCP server message bus agents; openai swarm handoff; autogen
group chat runtime; langgraph multi-agent; agent interop protocol 2026.

For EVERY candidate found, one table row: name | what it is (1 line) | wire
mechanism (file/socket/HTTP/tmux/MCP/protocol) | maturity (stars/date if shown)
| URL | verdict-for-us (adopt / steal-pattern / ignore, 1 clause). Below the
table, a short paragraph per non-ignored candidate: what exactly we would take.
No one-line dismissals of adoptable libraries; the build-vs-buy law requires
written per-candidate analysis before any bespoke build is recommended.

## REPORT.md required sections

1. Base verification (the ff-only receipt).
2. Prior-art table + per-candidate paragraphs (the sweep above).
3. Design: the recommended bus, mapped move-by-move onto surfaces that already
   ship (cite the ground-truth files by line for every local claim, URL for
   every internet claim). State the envelope schema, the wake path per harness,
   and the failure story (dropped line, dead session, two writers).
4. Verdict on the three Claude-side subagent workarounds (babysitter / MCP
   wrapper / litellm) in light of the prior art found.
5. Open questions you could not settle, each with what evidence would settle it.
6. Verbatim receipts: every fetched claim gets its URL; every local claim gets
   file:line.

Style: no em dashes; never the words provenance, substrate, load-bearing,
regime; descriptive names; terse.
