# Lane: bridge-research

Agent-bridging design plus internet prior-art sweep for messaging four coding-agent
harnesses (Claude Code, Codex CLI, opencode, kimi CLI) on one macOS box. Nothing was
built, nothing committed. Deliverable is this file.

## 1. Base verification

The first action was `git merge --ff-only 2eceb836`. It reported "Already up to date."
The branch `lane/bridge-research` already contained the base. Receipts:

- `git merge --ff-only 2eceb836` -> "Already up to date. MERGE_OK"
- `git merge-base --is-ancestor 2eceb836 HEAD` -> `BASE_IS_ANCESTOR`
- `HEAD = 2eceb8361029c140d2698ac25ce6e89336a215c8`
- `2eceb836` resolves to `2eceb8361029c140d2698ac25ce6e89336a215c8` (identical, no drift)

No deviation from the brief. Worktree root is
`/Users/chrishafley/projects/sprefa-lane-bridge`.

## 2. Prior-art table and per-candidate paragraphs

Sweep was run against hn.algolia.com/api/v1, api.github.com/repos, and direct fetches.
Two URL shapes were mined (HN Algolia JSON, GitHub API JSON). No fetch failed outright.
The keyword "agent interop protocol 2026" returned only off-topic medical/email noise
(FHIR, Matrix); logged and set aside.

### Table

| name | what it is | wire mechanism | maturity | URL | verdict-for-us |
|---|---|---|---|---|---|
| MCP Agent Mail (Dicklesworthstone) | "Gmail for coding agents": identities, inboxes, threaded messages, advisory file leases | MCP server (FastMCP stdio) + Git + SQLite | 2062 stars, active 2026-07-20 | https://github.com/Dicklesworthstone/mcp_agent_mail | steal-pattern: envelope, thread, lease, per-recipient inbox |
| mcp_agent_mail_rust | Rust port: 34 tools, Git-backed archive, SQLite index, TUI | MCP (stdio) + Git + SQLite | 116 stars, active 2026-08-02 | https://github.com/Dicklesworthstone/mcp_agent_mail_rust | steal-pattern: same, Rust-flavored (matches this repo's toolchain taste) |
| Beads (Yegge) | repo-native task graph for coding agents; issues agents pick up | files in repo (.beads/beads.jsonl) | referenced as SHIPPED by users; repo not API-verifiable here | https://github.com/steveyegge/beads | adopt-adjacent: pair with bus, not replace it |
| Beadhub (juanre) | agent chat + issue tracker around Beads, server-backed | HTTPS + Postgres | OSS MIT, "growing users" | https://beadhub.ai | ignore: adds a server + DB we do not need |
| Batty | supervised team of coding agents, test-gated, tmux-native | tmux panes + Maildir inboxes + JSONL logs + git worktrees | 47 stars, v0.1.0 | https://github.com/battysh/batty | adopt-pattern: Maildir-per-role + JSONL log + 5s poll loop audit trail |
| agent-channels (cheapsteak) | Slack-style channels for cross-session agent messaging | filesystem (claimed) | 0 stars, 2026-06-06 | https://github.com/cheapsteak/agent-channels | steal-pattern: cross-worktree channel file; negligible maturity so verify before borrowing |
| MatrixSwarm | "agent OS using only the filesystem": spawn agents, control swarms | filesystem | 0 stars, active 2026-02 | https://github.com/matrixswarm/matrixswarm | steal-pattern: file-as-message-plane; matches our zero-daemon bias conceptually |
| vibe-kanban | kanban board to run parallel coding agents per card | worktree + CLI per card | 27630 stars, active 2026-04-24 | https://github.com/BloopAI/vibe-kanban | steal-pattern: one session per task, worktree isolation |
| claude-squad | terminal app managing many Claude-Code-like agent instances | tmux + worktree | 8223 stars, active 2026-07-30 | https://github.com/smtg-ai/claude-squad | steal-pattern: session multiplex + worktree handling |
| Dagger container-use | run coding agents in parallel inside git-tracked containers | MCP podman/Docker + git branches | 3924 stars, active 2026-06-12 | https://github.com/dagger/container-use | ignore for wire; useful later for isolating parallel workers |
| OpenAI Swarm | educational handoff-based multi-agent framework | in-process Python, handoff objects | 21870 stars, archived/educational | https://github.com/openai/swarm | ignore: in-process, not cross-harness |
| microsoft/autogen | agent framework with group chat runtime | in-process AMP/groupchat | 60172 stars | https://github.com/microsoft/autogen | ignore: embeds its own agents, no CLI harness reuse |
| LangGraph supervisor | multi-agent orchestration graph | in-process Python | not API-verifiable here | https://github.com/langchain-ai/langgraph-supervisor | ignore: in-process orchestration, not our problem |
| Google A2A | agent-to-agent protocol (HTTP, JSON, agent cards) | HTTPS REST | industry protocol (Linux Foundation) | https://a2aprotocol.ai/ | steal-framing: envelope/role naming direction; too heavy for local file bus |
| ACP (IBM/zed) | agent client protocol: UI talks to an agent runtime over a wire | stdio JSON / JSON-RPC-ish | merged into A2A, being wound down | https://agentclientprotocol.com/overview/introduction | ignore as a target; note JSON-RPC framing confirms our upgrade path is standard |
| Relaymux | tmux-based meta-harness for local coding agents | tmux | new (2026-06) | https://github.com/mupt-ai/relaymux | steal-pattern: tmux hosting per-session agents |
| Orc | multi-agent orchestration in pure bash | bash + tmux | new (2026-03) | https://github.com/spencermarx/orc | steal-pattern: bash-first delivery loop style |
| Agent Conductor | CLI orchestrator for multi-agent tmux sessions | tmux | small | https://github.com/gaurav-yadav/agent-conductor | steal-pattern: per-session tmux orchestration |
| Agents Council | MCP server as a shared message bus, summons agents into a debate | MCP stdio, local state JSON | 3 pts on HN | https://github.com/MrLesk/agents-council | steal-pattern: cross-harness bus over MCP; small |
| Claude Code Agent Farm | parallel raw Claude Code instances from one terminal | tmux/server | small | https://github.com/Dicklesworthstone/claude_code_agent_farm | steal-pattern: session density |
| Rayline | routes Claude Code subagents to on-device/cheaper models | gateway/proxy on subagent model | 11 pts on HN | https://rayline.ai/ | relevant evidence for workaround verdict (see section 4) |
| DeepClaude | Claude Code agent loop with DeepSeek V4 Pro | gateway/proxy under Anthropic-style harness | 678 pts, 2026-05 | https://github.com/aattaran/deepclaude | evidence for workaround verdict |
| Contextify | indexes every Claude Code and Codex session into one SQLite FTS corpus, cross-harness search, MCP server | filesystem watch + SQLite FTS + MCP | 7 pts HN | https://contextify.sh/ | steal-pattern: transcript corpus as a shared read plane (parallels cass/S4) |
| schipper.ai "Parallel coding agents with tmux and Markdown specs" | human-routed parallel spec-driven agents, 4-8 at a time | tmux windows + Markdown FD specs + slash commands | article 2026-02-26 | https://schipper.ai/posts/parallel-coding-agents/ | steal-pattern: role-named tmux windows + file-spec handoff; note human is the router there |

### Per-candidate paragraphs (non-ignored)

**MCP Agent Mail / mcp_agent_mail_rust.** This is the strongest precedent and is the
exact "Agent Mail" that cass's `swarm` surface name-drops (cass reports its
`agent_mail` live provider as not wired). The design is ours in shape: per-recipient
inboxes, threaded envelopes, ack/leases. What we would take from it: the envelope
fields (from/to, thread id, reply-to), the per-recipient inbox as the unit of delivery,
and the advisory file-lease idea so workers can claim a file region and avoid clobbering
each other. The gap that makes us not adopt it wholesale: it assumes every participant
is an MCP client reachable through FastMCP over SQLite/Git, but our four harnesses have
no shared MCP surface and Claude Code cannot host arbitrary subagents. Our harnesses
inject by CLI. So we steal its message-plane vocabulary and keep our own tiny delivery
loop (section 3).

**Batty.** Directly validates the file-plane approach for cross-harness comms: role per
tmux pane, Maildir inboxes, JSONL logs, a synchronous 5-second poll loop, and git
worktrees for isolation. All five of our harnesses fit "a session in a window." We take
its Maildir-per-role shape, its JSONL log for audit, and its explicit "not
fire-and-forget, supervise" stance. Its poll loop (5s) is simpler than our tail -f but
slower; our Claude leg uses Monitor (<50ms) so this is a per-harness trade, not a
design fork.

**agent-channels / MatrixSwarm.** Both confirm "plain files as the message plane" is an
established, credible pattern, which is the central assumption of the S1 mailbox.
Both are tiny (0 stars) so we borrow the concept, not the code. MatrixSwarm's framing
(spawn agents, control swarms, all over the filesystem) is the closest existing
expression of our intent.

**vibe-kanban / claude-squad / Relaymux / Orc / Agent Conductor / Claude Code Agent
Farm.** A large branch of the ecosystem solves the same single-machine multi-agent
problem with tmux + worktrees but routes through a human or a controller. They add a
session manager we would have to adopt on top of our harnesses; we already have four
harnesses that ship their own session model. We take their worktree-per-session and
role-named-window conventions so that a worker has an isolated checkout, but we do not
bring in a fifth controller surface. The bus stays orthogonal: it moves messages
between the harnesses we already run.

**Contextify.** Watches `~/.claude/projects/` and `~/.codex/sessions/`, parses each
transcript format, and ingests every turn into SQLite with full-text search, exposed
through a CLI and an MCP server. It is a one-machine instantiation of exactly the shared
read plane that cass already gives us at larger scope (cass covers four harnesses,
haiku:154). We take its confirmation that transcript-corpus-as-handoff is a working,
demanded pattern, and treat it as a fallback/variant of S4 rather than something to
build: cass already ships this role.

**schipper.ai.** Not a tool, a workflow, and highly convergent with our ground truth: a
planner writes a Markdown spec, a worker implements from it, a PM grooms, all in
role-named tmux windows, with git-tracked spec files as the handoff and slash commands
as the glue. This is exactly the S1/S5 pattern minus the automation: there the human is
the router who moves the FD from planner to worker. Our delivery daemon (S3) is the
piece that removes the human from that router seat. We take the role-named window
convention and the "spec/spec-ref in the handoff, not the full context" idea; that maps
onto S4 (bus carries a cass pointer, not pasted context).

**Google A2A / ACP.** The industry protocols point the same direction on framing (agent
identity, roles, request/response correlation) and ACP's JSON-RPC basis confirms the
upgrade path already named in our ground truth (JSON-RPC/LSP framing). But A2A is a
HTTPS card/agent-card protocol aimed at cross-machine, cross-org agents; for four
co-located CLI harnesses it is the wrong transport. We borrow its envelope vocabulary
(id, from, to, reply-to) and keep files as transport.

**OpenAI Swarm / autogen / LangGraph.** All in-process or framework-embedded multi-agent
runtimes. They cannot reuse the four CLI harnesses as-is, so they fail the "map onto
surfaces that already ship" test and are ignored for the bus.

## 3. Design: recommended bus

Recommended: adopt seeds S1 + S3 (mailbox + single delivery daemon), with S4 (cass as
the carried pointer plane) and S5 (registry) folded in. S2 (per-harness delivery loop)
collapses into S3: one daemon owns delivery to all harnesses except Claude Code, which
wakes itself via Monitor.

Ground-truth claims, each cited below:
- cass is installed, indexes all four harnesses' transcripts, and exposes
  `search/pack --robot --json` plus `index --watch` (haiku-comms-research.md:154,
  :179-204, :216-217).
- Injection into a live/resumable session works per harness:
  `opencode run -s <sessionID> "msg"` (haiku:369; chat_log 20260802.2:21) and
  `kimi -S <uuid> -p "msg"` (haiku:389; chat_log:21).
- Claude Code wakes on file change via the Monitor tool on `tail -f` of a buffer, under
  50ms per line, persistent across the session (haiku:95, :112).
- The ruled NDJSON protocol precedent: id-tagged jobs, `{id,done}` terminator, cancel
  line, JSON-RPC/LSP named as the upgrade path (chat_log:15).
- Claude Code has no native non-Claude subagent (haiku:15-23), with three workarounds
  previously listed: babysitter agent, MCP wrapper, litellm gateway (haiku:56, :71-75;
  chat_log:22).

Local read-only cass checks run for this report:
- `cass --version` -> 0.6.22.
- `cass robot-docs sources` lists only agent-session connectors; no mailbox path, so
  cass does not index `.agent/mail/*.ndjson` today.
- `cass swarm status --json` returns `"provider agent_mail ... live-provider-unimplemented"`
  and the `agent-mail`/`agentmail`/`beads` binaries are not on PATH. So the "Agent Mail"
  that cass references is not installed; it is the MCP Agent Mail precedent, external.

### Envelope schema (S1)

One JSON object per line, append-only, in a per-recipient file `<maildir>/<to>.ndjson`.
Mirrors the ruled NDJSON protocol (chat_log:15):

```
{ "id": "msg-<uuid>", "from": "fable", "to": "flash",
  "ts": "<iso8601>", "kind": "request|result|note",
  "reply_to": null,          // id of the message this answers (correlation)
  "body": "<text for the injected turn>",
  "ref": null }              // optional pointer, e.g. cass pack file/query (S4)
```

`kind: request|result|note` covers handoff, reply, and side-note. `reply_to` does
blocking request/reply correlation without a synchronous link: the daemon or the
coordinator can pair `result.reply_to == request.id`. A `result` with `done: true`
carries the `{id,done}` terminator semantics from the precedent.

### Wake path per harness (S2+S3 folded)

- Claude Code (planner): Monitor armed at session start on
  `tail -f <maildir>/fable.ndjson | grep --line-buffered .` (haiku:112). Each line wakes
  the model under 50ms. No daemon involvement for the planner's inbox.
- opencode worker: daemon runs `opencode run -s <sessionID> "msg"` (haiku:369).
- kimi worker: daemon runs `kimi -S <uuid> -p "msg"` (haiku:389).
- Codex agent: wake path not established in ground truth; see open question O1.

### The daemon (S3)

One ~30-line watcher (tail -F or fswatch) that:
1. Per recipient file, tracks last-delivered byte offset so a restart resumes where it
   stopped (at-least-once). No line is injected twice because delivery is recorded
   (byte offset + seen msg-ids).
2. Routes each new line to the recipient's inject command from the registry (S5).
3. Rewrites liveness in the registry on nonzero exit or dead session (failure story).

Registry (S5): `<maildir>/registry.json` maps agent name -> harness, session id, inject
command, liveness, last-acked offset.

### Failure story

- Dropped line: at-least-once. The daemon persists the delivered-byte-offset per
  mailbox and the set of delivered msg-ids; on crash/restart it resumes from the stored
  offset and skips ids already delivered. Fire-and-forget is rejected; every request is
  expected to produce a `result` envelope, which doubles as the ack and the correlation
  handle. This is the ack-free, at-least-once contract, chosen because no harness gives
  us a transactional inject.
- Dead session: the inject command fails or the registry's liveness is stale. The
  daemon marks the agent down and emits a `result`-shaped `note` back to the sender
  (`"Delivery failed: <agent> unavailable"`), so the coordinator is never left hanging
  silently.
- Two writers: one recipient, one mailbox file, append-only with O_APPEND and one
  writer per line; the daemon is the only consumer. If two harnesses could both write a
  single agent's mailbox (not the design), a per-file advisory lock serializes appends.
  The daemon is single-process, so there is one consumer regardless.

### S4 (cass as pointer plane)

Bus envelopes carry a `ref` (e.g. a `cass pack "topic" --robot --json` query/result)
instead of pasted context (haiku:179-204, :429). `cass index --watch` (haiku:216-217)
keeps the shared read plane fresh across all four harnesses. The bus moves pointers, not
payloads; the recipient pulls the pack.

## 4. Verdict on the three Claude-side subagent workarounds

Prior art materially changes the light on each.

1. Babysitter agent (`.claude/agents/opencode-lane.md`, a Claude model shelling
   `opencode run --format json`). Prior art shows "Claude Code driving a different
   model" is a worked, shipping pattern (Rayline routes Claude Code subagents to
   on-device/cheaper models; DeepClaude pairs DeepSeek under the Claude loop). This
   gives a real panel row while preserving the external harness's own agent loop
   (haiku:71). Verdict: still viable, and now corroborated, but it is scoped to "Claude
   wants a deepseek worker as a panel row," not to the bus. The bus does not need it.

2. MCP wrapper (thin stdio server exposing `run_opencode_task` / `run_kimi_task`).
   Prior art makes this the ecosystem's default cross-harness integration: MCP Agent
   Mail, Agents Council, and Dagger container-use all wire agents together over MCP.
   Workers run unmodified and appear as tool calls, no panel row (haiku:72). Verdict:
   this is the battle-tested way to give Claude Code a mailbox tool (read/write
   envelopes) and is the natural upgrade if Monitor proves fragile. For the initial bus
   we keep Claude on Monitor (already ship, under 50ms) and let the daemon inject to
   opencode/kimi by CLI, so the wrapper is a fallback, not the foundation.

3. litellm/gateway (Anthropic Messages API emulator fronting a non-Claude model,
   referenced by subagent `model:`). Prior art confirms the pattern is real and
   achievable (Rayline, claude-code-router family, DeepClaude) and it is the only way to
   get a real panel row with a genuinely different model answering (haiku:73). But the
   sweep also confirms it discards the worker's own harness: Claude's tool loop drives,
   the external model only completes (haiku:40, :57). Verdict: keep as the last-resort
   tool only for the narrow "I want a deepseek panel row" goal. Rejected for the bus,
   whose entire point is harness independence: the bus must let each harness run under
   its own engine.

Net: the bus does not require any of the three. It is the fourth option named in the
open question (chat_log:52) and the prior art (MCP Agent Mail, Batty, the file-plane
projects) says the mailbox bus is the standard answer for cross-harness messaging, with
the three workarounds reserved for the in-UI visibility problem they actually solve.

## 5. Open questions not settled, and the evidence that would settle each

- O1 Codex CLI inbound injection. Ground truth covers Claude/opencode/kimi only. What
  flag resumes a live/resumable Codex session and injects a turn, non-interactively?
  Evidence: `codex --help` / docs / `~/.codex/sessions` layout; a harness table like
  haiku:367-397 for Codex. Settles whether the daemon has a fourth leg or Codex rides
  cass read-only for now.
- O2 repo vs ~/.agent mailbox location. Ground truth uses in-repo `.agent/` paths
  (chat_log:21). The box runs many repos (multi-repo), so the question is per-project
  mail dirs vs one global `~/.agent/mail`. Evidence: whether any handoff crosses repo
  boundaries; a global dir supports the latter, `.agent/` supports per-project scoping
  and self-contained worktrees. A decision rule: if a coordinator sends to a worker in
  another worktree of the same repo, `.agent/mail` works; if it sends across repos to a
  repo that is not the sender's, a shared `~/.agent/mail` is needed.
- O3 Does a harness inject new sessions or only resume? For a worker that is not
  running, the daemon currently cannot route. Whether to cold-start a fresh session
  (`opencode run` with no `-s`) vs require a live session is unsettled. Evidence: a
  probe of `opencode run`/`kimi -S` without a session id on this box; deciding the
  semantics of "spawn a missing worker" (which may need a supervisor to build the right
  prompt). This is the "who spawns a worker that is not running" item.
- O4 Blocking request/reply. reply_to gives correlation, but there is no synchronous
  wait. Whether the coordinator should block (poll its own inbox for a matching result)
  or fire-and-forget top-of-queue is a product choice. Evidence: latency measurement of
  one inject-to-wake-to-result round trip per harness; if round trips are fast, blocking
  is cheap; if a worker only wakes on its next user turn, blocking degrades and async
  note-style becomes mandatory.
- O5 Does cass index the mailboxes now, and can it? This report establishes it does not
  (connectors are fixed session dirs) and that its `agent_mail` swarm provider is
  unwired. Evidence for "could it": whether cass has a generic NDJSON/JSONL source
  connector or a `sources` extension point for arbitrary mail dirs; `cass robot-docs
  sources` and `cass introspect`. If not supported, mailboxes stay outside cass and S4
  (cass as pointer plane) still works because cass indexes the harness transcripts the
  pointers refer to.
- O6 Ack vs at-least-once + result envelope. Chosen here: at-least-once plus a required
  `result` envelope. Settling it further needs a probe of whether any inject produces a
  deterministic completion signal (opencode/kimi `--format json` exit status) the daemon
  can trap as the ack, replacing offset-based dedup with an explicit per-message ack.
  Evidence: inspect the `--format json` / non-interactive output contract of
  `opencode run` and `kimi -p`.

## 6. Verbatim receipts

Local claims (file:line):

- haiku-comms-research.md (recovered 2026-08-02, 3 task reports + synthesis):
  - cass v0.6.22, 1,871 conversations indexed across harnesses: haiku:154
  - harness session source paths (claude_code, codex, opencode, kimi): haiku:160-165
  - search/pack `--robot --json`, pipelines, token budgets: haiku:179-205
  - `cass index --watch` / `--watch-interval 30`: haiku:216-217, :279-282
  - swarm status + "Beads + Agent Mail" external orchestration: haiku:311-315
  - Monitor tool `tail -f` wake: table haiku:95; recommended wiring haiku:112; SessionStart
    hook caveat haiku:129
  - opencode inject `opencode run -s <sessionID> "msg"`: haiku:369
  - opencode session storage ~/.local/share/opencode/opencode.db, ses_ format: haiku:372-375
  - kimi inject `kimi -S <sessionID> -p "message"`: haiku:389
  - kimi session storage wire.jsonl: haiku:392-394
  - no native non-Claude subagent, model field Claude-only: haiku:17-23
  - babysitter/MCP/gateway ranked: haiku:56, :71-75; MCP wrapper cost haiku:72; gateway
    discards worker loop haiku:40, :57
- chat_log/20260802.2.cst-rulings-duels-schemagen-alloy.md:
  - ruled NDJSON protocol, id-tagged jobs, {id,done}, cancel, JSON-RPC/LSP upgrade: line 15
  - comms bus summary, inject opencode/kimi, Monitor wake, ~30 LOC not built: line 21
  - native-subagent workarounds not chosen: line 22
  - open question "wire the mailbox bus instead?": line 52
- Local cass run (this session, read-only):
  - `cass --version` -> 0.6.22
  - `cass robot-docs sources` -> lists only agent-session connectors; no mail source
  - `cass swarm status --json` -> `"agent_mail"` provider `live-provider-unimplemented`
  - `which agent-mail agentmail beads` -> not found

Internet claims (URL):

- MCP Agent Mail repo + Gmail-for-agents pitch, 2025-10-27: https://github.com/Dicklesworthstone/mcp_agent_mail (stars via https://api.github.com/repos/Dicklesworthstone/mcp_agent_mail)
- mcp_agent_mail_rust: https://github.com/Dicklesworthstone/mcp_agent_mail_rust (stars via GitHub API)
- Beads by Steve Yegge (referenced in HN comment): https://github.com/steveyegge/beads and https://beadhub.ai
- Batty (Maildir inboxes, JSONL logs, 5s poll): https://github.com/battysh/batty (stars via GitHub API)
- agent-channels: https://github.com/cheapsteak/agent-channels
- MatrixSwarm (filesystem agent OS): https://github.com/matrixswarm/matrixswarm
- vibe-kanban: https://github.com/BloopAI/vibe-kanban (27630 stars)
- claude-squad: https://github.com/smtg-ai/claude-squad (8223 stars)
- Dagger container-use: https://github.com/dagger/container-use (3924 stars) + https://dagger.io/blog/agent-container-use
- OpenAI Swarm: https://github.com/openai/swarm (21870 stars)
- microsoft/autogen: https://github.com/microsoft/autogen (60172 stars)
- LangGraph supervisor: https://github.com/langchain-ai/langgraph-supervisor (stars not retrievable via unauthenticated API on this run)
- Google A2A: https://a2aprotocol.ai/
- ACP joining A2A under Linux Foundation: https://lfaidata.foundation/communityblog/2025/08/29/acp-joins-forces-with-a2a-under-the-linux-foundations-lf-ai-data/
- schipper.ai parallel agents with tmux and Markdown specs, 2026-02-26: https://schipper.ai/posts/parallel-coding-agents/ and HN https://news.ycombinator.com/item?id=47218318
- Agents Council (MCP message bus): https://github.com/MrLesk/agents-council
- Rayline (Claude Code subagents to cheaper models): https://rayline.ai/
- DeepClaude (Claude Code agent loop with DeepSeek V4 Pro): https://github.com/aattaran/deepclaude (HN: https://news.ycombinator.com/item?id=48002136)
- Contextify (transcript corpus, cross-harness search, MCP server, parallels cass): https://contextify.sh/ (HN: https://news.ycombinator.com/item?id=48777790)
- Relaymux: https://github.com/mupt-ai/relaymux
- Orc: https://github.com/spencermarx/orc
- Claude Code Agent Farm: https://github.com/Dicklesworthstone/claude_code_agent_farm
- Agent Conductor: https://github.com/gaurav-yadav/agent-conductor

Note: steveyegge/beads and langchain-ai/langgraph-supervisor returned no fields from the
unauthenticated GitHub API on this run (likely rate-limit/redirect); both are marked
"not API-verifiable here" in the table and their URLs are retained as references.
